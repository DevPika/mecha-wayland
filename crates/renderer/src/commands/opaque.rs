use std::mem::size_of;

use glow::{HasContext, NativeBuffer, NativeProgram, NativeUniformLocation};
use utils::{Color, Point, Rect, Size};

use crate::{commands::RenderContext, texture::TextureId};

#[derive(Clone)]
pub(crate) struct OpaquePrim {
    pub origin: Point,
    pub z: f32,
    pub size: Size,
    pub fg: Color,
    pub bg: Color,
    pub region: Rect,
    pub texture_id: Option<TextureId>,
}

const FLOATS_PER_VERTEX: usize = 19;

fn push_prim_verts(v: &mut Vec<f32>, p: &OpaquePrim) {
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
    for (px, py) in CORNERS {
        v.extend_from_slice(&[
            px, py, p.fg.r, p.fg.g, p.fg.b, p.fg.a, p.bg.r, p.bg.g, p.bg.b, p.bg.a, ox, oy, oz, sw,
            sh, rx, ry, rw, rh,
        ]);
    }
}

#[derive(Default)]
pub(crate) struct OpaqueQueue {
    shader_program: Option<NativeProgram>,
    vbo: Option<NativeBuffer>,
    u_viewport_inv_res_loc: Option<NativeUniformLocation>,
    u_tex_inv_size_loc: Option<NativeUniformLocation>,
    u_texture_loc: Option<NativeUniformLocation>,
    u_tex_weight_loc: Option<NativeUniformLocation>,
    rects: Vec<OpaquePrim>,
    sprites: Vec<OpaquePrim>,
}

impl OpaqueQueue {
    pub(crate) fn push_rect(&mut self, prim: OpaquePrim) {
        self.rects.push(prim);
    }

    pub(crate) fn push_sprite(&mut self, prim: OpaquePrim) {
        self.sprites.push(prim);
    }

    pub(crate) fn init(&mut self, ctx: &RenderContext) {
        unsafe {
            let gl = ctx.gl;
            let program = gl.create_program().expect("glCreateProgram");

            // Lean: no SDF. A solid textured/coloured quad that always writes
            // opaque pixels. Rects sample nothing (uTexWeight = 0); opaque
            // sprites composite glyph coverage over a known background.
            let vs_src = r#"#version 100
                attribute vec2 aPos;
                attribute vec4 aFg;
                attribute vec4 aBg;
                attribute vec3 aOrigin;
                attribute vec2 aSize;
                attribute vec4 aRegion;

                varying vec4 vFg;
                varying vec4 vBg;
                varying vec2 vUV;

                uniform vec2 uViewportInvRes;
                uniform vec2 uTexInvSize;

                void main() {
                    vec2 center   = aOrigin.xy + aSize * 0.5;
                    vec2 pixelPos = vec2(aPos.x, -aPos.y) * aSize + center;
                    vec2 ndc      = pixelPos * uViewportInvRes - 1.0;
                    gl_Position   = vec4(ndc, aOrigin.z, 1.0);

                    vFg = aFg;
                    vBg = aBg;

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

            let fs_src = r#"#version 100
                precision mediump float;

                varying vec4 vFg;
                varying vec4 vBg;
                varying vec2 vUV;

                uniform sampler2D uTexture;
                uniform float uTexWeight;

                void main() {
                    float mask = 1.0;
                    if (uTexWeight > 0.5) {
                        mask = texture2D(uTexture, vUV).r;
                    }
                    gl_FragColor = vec4(mix(vBg.rgb, vFg.rgb, mask), 1.0);
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
            gl.bind_attrib_location(program, 1, "aFg");
            gl.bind_attrib_location(program, 2, "aBg");
            gl.bind_attrib_location(program, 3, "aOrigin");
            gl.bind_attrib_location(program, 4, "aSize");
            gl.bind_attrib_location(program, 5, "aRegion");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("shader link error: {}", gl.get_program_info_log(program));
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            self.u_viewport_inv_res_loc = gl.get_uniform_location(program, "uViewportInvRes");
            self.u_tex_inv_size_loc = gl.get_uniform_location(program, "uTexInvSize");
            self.u_texture_loc = gl.get_uniform_location(program, "uTexture");
            self.u_tex_weight_loc = gl.get_uniform_location(program, "uTexWeight");
            self.shader_program = Some(program);
            self.vbo = Some(gl.create_buffer().expect("glCreateBuffer"));
        }
    }

    pub(crate) fn process(&mut self, ctx: &RenderContext) {
        let mut rects = std::mem::take(&mut self.rects);
        let mut sprites = std::mem::take(&mut self.sprites);
        if rects.is_empty() && sprites.is_empty() {
            return;
        }
        let Some(program) = self.shader_program else {
            return;
        };

        let gl = ctx.gl;
        let vp_w = ctx.viewport_width as f32;
        let vp_h = ctx.viewport_height as f32;
        const STRIDE: i32 = (FLOATS_PER_VERTEX * size_of::<f32>()) as i32;

        unsafe {
            gl.use_program(Some(program));
            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo);
            for i in 0..=5 {
                gl.enable_vertex_attrib_array(i);
            }
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, STRIDE, 0);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, STRIDE, 2 * 4);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, STRIDE, 6 * 4);
            gl.vertex_attrib_pointer_f32(3, 3, glow::FLOAT, false, STRIDE, 10 * 4);
            gl.vertex_attrib_pointer_f32(4, 2, glow::FLOAT, false, STRIDE, 13 * 4);
            gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, STRIDE, 15 * 4);

            gl.uniform_2_f32(self.u_viewport_inv_res_loc.as_ref(), 2.0 / vp_w, 2.0 / vp_h);
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(self.u_texture_loc.as_ref(), 0);

            // Opaque: write depth, no blend. Front-to-back for early-z.
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::GEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);

            // Rects: one untextured draw (uTexWeight = 0).
            if !rects.is_empty() {
                rects.sort_unstable_by(|a, b| b.z.total_cmp(&a.z));
                gl.uniform_1_f32(self.u_tex_weight_loc.as_ref(), 0.0);
                gl.bind_texture(glow::TEXTURE_2D, None);
                let mut verts = Vec::with_capacity(rects.len() * 6 * FLOATS_PER_VERTEX);
                for p in &rects {
                    push_prim_verts(&mut verts, p);
                }
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(&verts),
                    glow::STREAM_DRAW,
                );
                gl.draw_arrays(glow::TRIANGLES, 0, (rects.len() * 6) as i32);
            }

            // Opaque sprites: textured, batched by atlas (uTexWeight = 1).
            if !sprites.is_empty() {
                sprites.sort_unstable_by(|a, b| {
                    b.z.total_cmp(&a.z)
                        .then(a.texture_id.map(|t| t.0).cmp(&b.texture_id.map(|t| t.0)))
                });
                gl.uniform_1_f32(self.u_tex_weight_loc.as_ref(), 1.0);

                let mut i = 0usize;
                while i < sprites.len() {
                    let tex_id = sprites[i].texture_id;
                    let mut end = i + 1;
                    while end < sprites.len() && sprites[end].texture_id == tex_id {
                        end += 1;
                    }
                    let Some(gpu) = tex_id.and_then(|id| ctx.textures.get(&id)) else {
                        i = end;
                        continue;
                    };
                    gl.bind_texture(glow::TEXTURE_2D, Some(gpu.handle));
                    gl.uniform_2_f32(
                        self.u_tex_inv_size_loc.as_ref(),
                        1.0 / gpu.width as f32,
                        1.0 / gpu.height as f32,
                    );
                    let batch = &sprites[i..end];
                    let mut verts = Vec::with_capacity(batch.len() * 6 * FLOATS_PER_VERTEX);
                    for p in batch {
                        push_prim_verts(&mut verts, p);
                    }
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        bytemuck::cast_slice(&verts),
                        glow::STREAM_DRAW,
                    );
                    gl.draw_arrays(glow::TRIANGLES, 0, (batch.len() * 6) as i32);
                    i = end;
                }
            }

            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);
            gl.bind_texture(glow::TEXTURE_2D, None);
            for i in 0..=5 {
                gl.disable_vertex_attrib_array(i);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);
        }
    }
}
