#![forbid(unsafe_code)]

use app::prelude::*;

mod time;

pub use time::Time;

pub struct AnimationModule;
impl Module for AnimationModule {
    fn install(self, app: &mut App) {
        app.init_resource::<Time>()
            .system::<PostTick>(time::update_time)
            .system::<window::Frame>(time::update_time);
    }
}
