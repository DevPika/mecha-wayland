use std::mem::size_of;

use glow::{HasContext, NativeBuffer, NativeProgram, NativeUniformLocation};
use utils::{Color, Point, Rect, Size};

use crate::{commands::RenderContext, texture::TextureId};

#[derive(Clone)]
pub(crate) struct TranslucentPrim {
    pub origin: Point,
    pub z: f32,
    pub size: Size,
    pub color: Color,
    pub border_color: Color,
    pub border_radius: f32,
    pub border_thickness: f32,
    pub region: Rect,
    pub texture_id: Option<TextureId>,
    pub seq: u32,
}

const FLOATS_PER_VERTEX: usize = 22;

fn push_prim_verts(v: &mut Vec<f32>, p: &TranslucentPrim) {
    #[rustfmt::skip]
    const CORNERS: [(f32, f32); 6] = [
        (-0.5,  0.5), ( 0.5,  0.5), (-0.5, -0.5),
        ( 0.5,  0.5), ( 0.5, -0.5), (-0.5, -0.5),
    ];
    let (ox, oy) = p.origin.as_tuple();
    let oz = p.z;
    let (sw, sh) = p.size.as_tuple();
    let (rx, ry, rw, rh) = (
        p.region.x(),
        p.region.y(),
        p.region.width(),
        p.region.height(),
    );
    // Quads (no texture) carry weight 0 → mask forced to 1, so the (harmless)
    // atlas sample is discarded and quad + sprite share one draw call.
    let tex_weight = if p.texture_id.is_some() { 1.0 } else { 0.0 };
    for (px, py) in CORNERS {
        v.extend_from_slice(&[
            px,
            py,
            p.color.r,
            p.color.g,
            p.color.b,
            p.color.a,
            p.border_color.r,
            p.border_color.g,
            p.border_color.b,
            p.border_color.a,
            ox,
            oy,
            oz,
            sw,
            sh,
            p.border_radius,
            p.border_thickness,
            rx,
            ry,
            rw,
            rh,
            tex_weight,
        ]);
    }
}

#[derive(Default)]
pub(crate) struct TranslucentQueue {
    shader_program: Option<NativeProgram>,
    vbo: Option<NativeBuffer>,
    u_viewport_inv_res_loc: Option<NativeUniformLocation>,
    u_tex_inv_size_loc: Option<NativeUniformLocation>,
    u_texture_loc: Option<NativeUniformLocation>,
    prims: Vec<TranslucentPrim>,
    next_seq: u32,
}

impl TranslucentQueue {
    /// Push a primitive, assigning it the next submission index.
    pub(crate) fn push(&mut self, mut prim: TranslucentPrim) {
        prim.seq = self.next_seq;
        self.next_seq += 1;
        self.prims.push(prim);
    }

    pub(crate) fn init(&mut self, ctx: &RenderContext) {
        unsafe {
            let gl = ctx.gl;
            let program = gl.create_program().expect("glCreateProgram");

            // GLSL ES 1.00 — device is GLES 2.0 (Vivante GC7000).
            let vs_src = r#"#version 100
                attribute vec2  aPos;
                attribute vec4  aColor;
                attribute vec4  aBorderColor;
                attribute vec3  aOrigin;
                attribute vec2  aSize;
                attribute float aBorderRadius;
                attribute float aBorderThickness;
                attribute vec4  aRegion;
                attribute float aTexWeight;

                varying vec4  vColor;
                varying vec4  vBorderColor;
                varying vec2  vLocalPos;
                varying vec2  vSize;
                varying float vBorderRadius;
                varying float vBorderThickness;
                varying vec2  vUV;
                varying float vTexWeight;

                uniform vec2 uViewportInvRes;
                uniform vec2 uTexInvSize;

                void main() {
                    vec2 center   = aOrigin.xy + aSize * 0.5;
                    vec2 pixelPos = vec2(aPos.x, -aPos.y) * aSize + center;
                    vec2 ndc      = pixelPos * uViewportInvRes - 1.0;
                    gl_Position   = vec4(ndc, aOrigin.z, 1.0);

                    vColor           = aColor;
                    vBorderColor     = aBorderColor;
                    vLocalPos        = aPos;
                    vSize            = aSize;
                    vBorderRadius    = aBorderRadius;
                    vBorderThickness = aBorderThickness;
                    vTexWeight       = aTexWeight;

                    vec2 uv_frac = vec2(aPos.x + 0.5, 0.5 - aPos.y);
                    vUV = (aRegion.xy + uv_frac * aRegion.zw) * uTexInvSize;
                }
            "#;

            let vs = gl
                .create_shader(glow::VERTEX_SHADER)
                .expect("glCreateShader(VERTEX_SHADER)");
            gl.shader_source(vs, vs_src);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                panic!(
                    "vertex shader compile error: {}",
                    gl.get_shader_info_log(vs)
                );
            }

            // Rounded-rect SDF + border, multiplied by an atlas mask. Quads carry
            // aTexWeight = 0 (mask = 1); sprites carry radius/thickness 0 so the
            // SDF collapses to a plain rect and the glyph mask does the shaping.
            let fs_src = r#"#version 100
                precision mediump float;

                varying vec4  vColor;
                varying vec4  vBorderColor;
                varying vec2  vLocalPos;
                varying vec2  vSize;
                varying float vBorderRadius;
                varying float vBorderThickness;
                varying vec2  vUV;
                varying float vTexWeight;

                uniform sampler2D uTexture;

                float roundedRectSDF(vec2 p, vec2 halfSize, float r) {
                    vec2 q = abs(p) - halfSize + r;
                    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
                }

                void main() {
                    vec2  p        = vLocalPos * vSize;
                    vec2  halfSize = vSize * 0.5;
                    float dist     = roundedRectSDF(p, halfSize, vBorderRadius);

                    float alpha    = 1.0 - smoothstep(-0.5, 0.5, dist);
                    float inBorder = smoothstep(-vBorderThickness - 0.5, -vBorderThickness + 0.5, dist);

                    vec4  color = mix(vColor, vBorderColor, inBorder);
                    float mask  = mix(1.0, texture2D(uTexture, vUV).r, vTexWeight);
                    gl_FragColor = vec4(color.rgb, color.a * alpha * mask);
                }
            "#;

            let fs = gl
                .create_shader(glow::FRAGMENT_SHADER)
                .expect("glCreateShader(FRAGMENT_SHADER)");
            gl.shader_source(fs, fs_src);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                panic!(
                    "fragment shader compile error: {}",
                    gl.get_shader_info_log(fs)
                );
            }

            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "aPos");
            gl.bind_attrib_location(program, 1, "aColor");
            gl.bind_attrib_location(program, 2, "aBorderColor");
            gl.bind_attrib_location(program, 3, "aOrigin");
            gl.bind_attrib_location(program, 4, "aSize");
            gl.bind_attrib_location(program, 5, "aBorderRadius");
            gl.bind_attrib_location(program, 6, "aBorderThickness");
            gl.bind_attrib_location(program, 7, "aRegion");
            gl.bind_attrib_location(program, 8, "aTexWeight");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("shader link error: {}", gl.get_program_info_log(program));
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            self.u_viewport_inv_res_loc = gl.get_uniform_location(program, "uViewportInvRes");
            self.u_tex_inv_size_loc = gl.get_uniform_location(program, "uTexInvSize");
            self.u_texture_loc = gl.get_uniform_location(program, "uTexture");
            self.shader_program = Some(program);
            self.vbo = Some(gl.create_buffer().expect("glCreateBuffer"));
        }
    }

    pub(crate) fn process(&mut self, ctx: &RenderContext) {
        let mut prims = std::mem::take(&mut self.prims);
        self.next_seq = 0;
        if prims.is_empty() {
            return;
        }
        let Some(program) = self.shader_program else {
            return;
        };

        prims.sort_unstable_by(|a, b| a.z.total_cmp(&b.z).then(a.seq.cmp(&b.seq)));

        let atlas = prims
            .iter()
            .find_map(|p| p.texture_id)
            .and_then(|id| ctx.textures.get(&id));

        let gl = ctx.gl;
        let vp_w = ctx.viewport_width as f32;
        let vp_h = ctx.viewport_height as f32;
        const STRIDE: i32 = (FLOATS_PER_VERTEX * size_of::<f32>()) as i32;

        let mut verts = Vec::with_capacity(prims.len() * 6 * FLOATS_PER_VERTEX);
        for p in &prims {
            push_prim_verts(&mut verts, p);
        }

        unsafe {
            gl.use_program(Some(program));
            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo);

            for i in 0..=8 {
                gl.enable_vertex_attrib_array(i);
            }
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, STRIDE, 0);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, STRIDE, 2 * 4);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, STRIDE, 6 * 4);
            gl.vertex_attrib_pointer_f32(3, 3, glow::FLOAT, false, STRIDE, 10 * 4);
            gl.vertex_attrib_pointer_f32(4, 2, glow::FLOAT, false, STRIDE, 13 * 4);
            gl.vertex_attrib_pointer_f32(5, 1, glow::FLOAT, false, STRIDE, 15 * 4);
            gl.vertex_attrib_pointer_f32(6, 1, glow::FLOAT, false, STRIDE, 16 * 4);
            gl.vertex_attrib_pointer_f32(7, 4, glow::FLOAT, false, STRIDE, 17 * 4);
            gl.vertex_attrib_pointer_f32(8, 1, glow::FLOAT, false, STRIDE, 21 * 4);

            gl.uniform_2_f32(self.u_viewport_inv_res_loc.as_ref(), 2.0 / vp_w, 2.0 / vp_h);

            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(self.u_texture_loc.as_ref(), 0);
            match atlas {
                Some(tex) => {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex.handle));
                    gl.uniform_2_f32(
                        self.u_tex_inv_size_loc.as_ref(),
                        1.0 / tex.width as f32,
                        1.0 / tex.height as f32,
                    );
                }
                None => {
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    gl.uniform_2_f32(self.u_tex_inv_size_loc.as_ref(), 0.0, 0.0);
                }
            }

            // Depth-test against the opaque pass (early-z); never write depth.
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::GEQUAL);
            gl.depth_mask(false);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );

            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::STREAM_DRAW,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, (prims.len() * 6) as i32);

            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.bind_texture(glow::TEXTURE_2D, None);
            for i in 0..=8 {
                gl.disable_vertex_attrib_array(i);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);
        }
    }
}
