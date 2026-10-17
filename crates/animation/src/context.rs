//! `AnimationContext`: read and replace a widget's own layout and paint
//! animation settings through its `Context`.

use app::{Context, Widget};

use crate::{LayoutAnimationSettings, PaintAnimationSettings};

/// Access the owner's animation settings, independently for layout and paint.
/// Getters return the node's own components, not settings inherited from an
/// ancestor. A default component means "inherit".
pub trait AnimationContext {
    fn layout_animation(&self) -> &LayoutAnimationSettings;
    fn paint_animation(&self) -> &PaintAnimationSettings;

    /// Replace this node's layout settings. Returns `true` after the write,
    /// even when the new value is identical to the old one.
    fn set_layout_animation(&mut self, settings: LayoutAnimationSettings) -> bool;

    /// Replace this node's paint settings. Returns `true` after the write,
    /// even when the new value is identical to the old one.
    fn set_paint_animation(&mut self, settings: PaintAnimationSettings) -> bool;
}

impl<W: Widget> AnimationContext for Context<'_, W> {
    fn layout_animation(&self) -> &LayoutAnimationSettings {
        self.component::<LayoutAnimationSettings>()
            .expect("AnimationModule is installed wherever animation::AnimationContext is used")
    }

    fn paint_animation(&self) -> &PaintAnimationSettings {
        self.component::<PaintAnimationSettings>()
            .expect("AnimationModule is installed wherever animation::AnimationContext is used")
    }

    fn set_layout_animation(&mut self, settings: LayoutAnimationSettings) -> bool {
        *self
            .component_mut::<LayoutAnimationSettings>()
            .expect("AnimationModule is installed wherever animation::AnimationContext is used") =
            settings;
        true
    }

    fn set_paint_animation(&mut self, settings: PaintAnimationSettings) -> bool {
        *self
            .component_mut::<PaintAnimationSettings>()
            .expect("AnimationModule is installed wherever animation::AnimationContext is used") =
            settings;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use app::prelude::*;
    use paint::PaintModule;

    use super::*;
    use crate::{AnimationModule, AnimationTime};

    struct Poke;
    impl Event for Poke {}

    struct Editable;
    struct EditableBuilder(Box<dyn FnMut(&mut Context<'_, Editable>)>);
    impl Build for EditableBuilder {
        type Widget = Editable;
    }
    impl Widget for Editable {
        type Builder = EditableBuilder;
        fn build(b: EditableBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            let mut action = b.0;
            s.on::<Poke>(me, move |ctx, _| action(ctx));
            Editable
        }
    }

    #[test]
    fn getters_read_own_settings_and_setters_write_independently() {
        let mut app = App::new();
        app.add_module(PaintModule).add_module(AnimationModule);
        let id = app.spawn(
            app.root(),
            EditableBuilder(Box::new(|ctx| {
                assert!(ctx.layout_animation().0.is_none());
                assert!(ctx.paint_animation().0.is_none());

                let layout = LayoutAnimationSettings::custom(
                    AnimationTime::Duration(Duration::from_millis(250)),
                    |t| t,
                );
                let paint = PaintAnimationSettings::custom(AnimationTime::Speed(2.0), |t| t * t);
                assert!(ctx.set_layout_animation(layout));
                assert!(ctx.paint_animation().0.is_none());
                assert!(matches!(
                    ctx.layout_animation().0.unwrap().time,
                    AnimationTime::Duration(d) if d == Duration::from_millis(250)
                ));
                assert!(ctx.set_paint_animation(paint));
                assert!(matches!(
                    ctx.paint_animation().0.unwrap().time,
                    AnimationTime::Speed(2.0)
                ));
                assert_eq!(ctx.paint_animation().0.unwrap().easing.resolve(0.5), 0.25);
                assert!(
                    ctx.set_layout_animation(layout),
                    "equal replacements still write"
                );
                assert!(
                    ctx.set_paint_animation(paint),
                    "equal replacements still write"
                );
            })),
        );
        app.emit(Poke, id);
        app.flush();
        assert!(matches!(
            app.component::<LayoutAnimationSettings>(id).unwrap().0.unwrap().time,
            AnimationTime::Duration(d) if d == Duration::from_millis(250)
        ));
        assert!(matches!(
            app.component::<PaintAnimationSettings>(id)
                .unwrap()
                .0
                .unwrap()
                .time,
            AnimationTime::Speed(2.0)
        ));
    }
}
