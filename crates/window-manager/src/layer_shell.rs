use crate::WindowManager;
use wayland::ZwlrLayerSurfaceV1Event;

pub fn module<S>() -> impl app::RegisteredModule<WindowManager, S> {
    app::Module::<WindowManager, _, _>::new().on(
        |wm: &mut WindowManager, event: &ZwlrLayerSurfaceV1Event| {
            let ZwlrLayerSurfaceV1Event::Configure {
                sender,
                serial,
                width,
                height,
            } = event
            else {
                return;
            };
            let Some(sender_id) = sender.object_id() else {
                return;
            };
            let Some(id) = wm.window_for_role(sender_id) else {
                return;
            };
            // Layer-shell sends 0 to mean "pick your own size" — fall back to
            // what we requested at spawn.
            let (stored_w, stored_h) = wm.window_dimensions(id).unwrap_or((*width, *height));
            let w = if *width == 0 { stored_w } else { *width };
            let h = if *height == 0 { stored_h } else { *height };
            wm.configure(id, *serial, w, h);
        },
    )
}
