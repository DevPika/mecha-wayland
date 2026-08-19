use crate::WindowManager;
use wayland::XdgSurfaceEvent;

pub fn module<S>() -> impl app::RegisteredModule<WindowManager, S> {
    app::Module::<WindowManager, _, _>::new().on(
        |wm: &mut WindowManager, event: &XdgSurfaceEvent| {
            let XdgSurfaceEvent::Configure { sender, serial } = event;
            // The sender's object id may be gone if we destroyed the window and
            // the compositor delivers a configure during teardown. Skip it.
            let Some(obj_id) = sender.object_id() else {
                return;
            };
            let Some(id) = wm.window_for_role(obj_id) else {
                return;
            };
            // xdg's configure carries no size; use what we asked for at spawn.
            let Some((w, h)) = wm.window_dimensions(id) else {
                return;
            };
            wm.configure(id, *serial, w, h);
        },
    )
}
