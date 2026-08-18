use std::os::fd::AsFd;

use renderer::commands::{
    ClearColor, Color, DrawMonochromeSprite, DrawQuad, DrawText, Size as RSize,
};
use renderer::{DmaBuf, Rect, RenderableSurface, Renderer, SurfaceBackend};
use ui::{Damage, RenderCommand};
use wayland::{Handle, ZwpLinuxBufferParamsV1Flags, ZwpLinuxDmabufV1};

use crate::window::Slot;

pub(crate) enum FramePlan {
    Skip,
    /// Draw into the back slot.
    Draw {
        /// Whether taffy must be recomputed (a `Layout`/full frame). A `Paint`
        /// frame reuses the existing layout, so this is `false`.
        relayout: bool,
        /// The region to repaint — this frame's damage unioned with what the
        /// back slot still owed (buffer age). Feeds the GPU scissor and
        /// `wl_surface.damage`. The full surface for a `Layout`/full frame.
        region: Rect,
        /// This frame's *own* damage (the full surface for a `Layout` frame),
        /// to be added to the other slot's owed region. Distinct from `region`,
        /// which also carries the back slot's caught-up debt.
        this_damage: Rect,
    },
}

/// Classify a frame from its effective damage `class` and the back slot's
/// `owed` region. `full` is the whole-surface rect. Pure — the buffer-age state
/// mutation is [`apply_owed`], the pixels are [`submit_scene`].
///
/// `None` always skips, **even when `owed_back` is non-empty**: a slot's debt is
/// only relevant on a frame that actually presents it, and a skipped frame
/// presents nothing, so the debt simply waits for the next drawing frame.
pub(crate) fn plan_frame(class: Damage, owed_back: Rect, full: Rect) -> FramePlan {
    match class {
        Damage::None => FramePlan::Skip,
        Damage::Paint(r) => FramePlan::Draw {
            relayout: false,
            region: r.union(owed_back),
            this_damage: r,
        },
        // Full ∪ owed = full; the other slot then owes a full-surface pixel
        // catch-up (it lacks pixels, not layout).
        Damage::Layout => FramePlan::Draw {
            relayout: true,
            region: full,
            this_damage: full,
        },
    }
}

pub(crate) fn apply_owed(owed: &mut [Rect; 2], back: usize, this_damage: Rect) {
    owed[back] = Rect::ZERO;
    let other = back ^ 1;
    owed[other] = owed[other].union(this_damage);
}

pub(crate) fn submit_scene<B: SurfaceBackend>(
    renderer: &mut Renderer,
    surface: &RenderableSurface<B>,
    clear_color: Color,
    commands: Vec<RenderCommand>,
    scissor: Option<Rect>,
) {
    renderer.active_surface(surface);
    renderer.set_scissor(scissor);
    renderer.send_command(ClearColor(clear_color));

    for cmd in commands {
        match cmd {
            RenderCommand::DrawQuad {
                color,
                border_color,
                origin,
                z,
                size,
                border_radius,
                border_thickness,
            } => {
                renderer.send_command(DrawQuad {
                    color,
                    border_color,
                    origin,
                    z,
                    size,
                    border_radius,
                    border_thickness,
                });
            }
            RenderCommand::DrawText {
                font,
                text,
                origin,
                z,
                color,
            } => {
                let texture_id = renderer.get_texture_id(font.atlas_id);
                renderer.send_command(DrawText {
                    font,
                    texture_id,
                    text,
                    origin,
                    z,
                    color,
                    // Opaque-text variant is opt-in from the UI; until that
                    // traversal lands, text stays on the translucent path.
                    background: Color::TRANSPARENT,
                });
            }
            RenderCommand::DrawMonochromeSprite {
                atlas_id,
                region,
                origin,
                z,
                size,
                color,
            } => {
                let texture_id = renderer.get_texture_id(atlas_id);
                renderer.send_command(DrawMonochromeSprite {
                    texture_id,
                    region: Rect::new(region.x, region.y, region.w, region.h),
                    origin,
                    z,
                    size: RSize::new(size.width(), size.height()),
                    color,
                    background: Color::TRANSPARENT,
                });
            }
            _ => {}
        }
    }

    renderer.render_frame();
    renderer.finish();
}

const DRM_FORMAT_ARGB8888: u32 = 0x3432_5241;

pub fn alloc_slots(
    renderer: &mut Renderer,
    dmabuf: &Handle<ZwpLinuxDmabufV1>,
    width: u32,
    height: u32,
) -> [Slot; 2] {
    std::array::from_fn(|_| alloc_slot(renderer, dmabuf, width, height))
}

fn alloc_slot(
    renderer: &mut Renderer,
    dmabuf: &Handle<ZwpLinuxDmabufV1>,
    width: u32,
    height: u32,
) -> Slot {
    let surface = renderer
        .create_surface::<DmaBuf>(width, height)
        .expect("DmaBuf surface allocation failed");

    let buffer = {
        let fd = surface.backend.prime_fd.as_fd();
        let stride = surface.backend.stride;
        let modifier = surface.backend.modifier;
        let params = dmabuf.create_params();
        params.add(
            fd,
            0,
            0,
            stride,
            (modifier >> 32) as u32,
            (modifier & 0xffff_ffff) as u32,
        );
        params.create_immed(
            width as i32,
            height as i32,
            DRM_FORMAT_ARGB8888,
            ZwpLinuxBufferParamsV1Flags::empty(),
        )
    };

    Slot {
        surface,
        buffer,
        released: true,
        owed: Rect::ZERO,
    }
}
