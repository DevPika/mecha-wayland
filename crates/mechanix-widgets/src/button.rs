use crate::color::{ColorSource, IntoColorSource};
use crate::state::{StateLayer, WidgetState};
use crate::text::{Text, TextContextExt, text};
use app::{Build, Context, Handle, Spawner, Widget};
use atlas::SpriteId;
use geometry::{Color, Corners, Insets};
use layout::{LayoutStyle, StyleContext, Val, px};
use std::ops::Deref;
use theme::{ColorRole, Shape, Spacing, SpawnerThemeExt, TextVariant, ThemeReader};
use widgets::{
    Button as NativeButton, ButtonBuilder as NativeButtonBuilder,
    ButtonContext as NativeButtonContext, ButtonProps as NativeButtonProps,
    ButtonStateStyle as NativeButtonStateStyle, Icon, IconContext,
};

/// Mechanix Design button variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Filled,
    Tonal,
    Outlined,
    Text,
}

impl ButtonVariant {
    #[inline]
    pub fn fg_role(self) -> ColorRole {
        match self {
            Self::Filled => ColorRole::OnPrimary,
            Self::Tonal => ColorRole::OnSecondaryContainer,
            Self::Outlined | Self::Text => ColorRole::Primary,
        }
    }
}

/// Standard button sizes according to Mechanix specifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

impl ButtonSize {
    #[inline]
    pub fn height(self) -> f32 {
        match self {
            Self::XSmall => 24.0,
            Self::Small => 32.0,
            Self::Medium => 40.0,
            Self::Large => 48.0,
            Self::XLarge => 56.0,
        }
    }

    #[inline]
    pub fn text_variant(self) -> TextVariant {
        match self {
            Self::XSmall => TextVariant::LabelSmall,
            Self::Small => TextVariant::LabelMedium,
            Self::Medium => TextVariant::LabelLarge,
            Self::Large => TextVariant::TitleMedium,
            Self::XLarge => TextVariant::TitleLarge,
        }
    }

    #[inline]
    pub fn shape(self) -> Shape {
        match self {
            Self::XSmall | Self::Small => Shape::ExtraSmall,
            Self::Medium | Self::Large => Shape::Small,
            Self::XLarge => Shape::Medium,
        }
    }

    #[inline]
    pub fn icon_size(self) -> f32 {
        match self {
            Self::XSmall => 12.0,
            Self::Small => 16.0,
            Self::Medium => 18.0,
            Self::Large => 22.0,
            Self::XLarge => 26.0,
        }
    }

    #[inline]
    pub fn padding(self) -> Insets<Val> {
        match self {
            Self::XSmall => Insets::new(
                px(Spacing::Space25.dp()),
                px(Spacing::Space100.dp()),
                px(Spacing::Space25.dp()),
                px(Spacing::Space100.dp()),
            ),
            Self::Small => Insets::new(
                px(Spacing::Space50.dp()),
                px(Spacing::Space150.dp()),
                px(Spacing::Space50.dp()),
                px(Spacing::Space150.dp()),
            ),
            Self::Medium => Insets::new(
                px(Spacing::Space100.dp()),
                px(Spacing::Space200.dp()),
                px(Spacing::Space100.dp()),
                px(Spacing::Space200.dp()),
            ),
            Self::Large => Insets::new(
                px(Spacing::Space150.dp()),
                px(Spacing::Space250.dp()),
                px(Spacing::Space150.dp()),
                px(Spacing::Space250.dp()),
            ),
            Self::XLarge => Insets::new(
                px(Spacing::Space200.dp()),
                px(Spacing::Space300.dp()),
                px(Spacing::Space200.dp()),
                px(Spacing::Space300.dp()),
            ),
        }
    }

    #[inline]
    pub fn gap(self) -> Val {
        match self {
            Self::XSmall => px(Spacing::Space50.dp()),
            Self::Small => px(Spacing::Space75.dp()),
            Self::Medium => px(Spacing::Space100.dp()),
            Self::Large => px(Spacing::Space125.dp()),
            Self::XLarge => px(Spacing::Space150.dp()),
        }
    }
}

/// Overrides for explicit styling outside the theme defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ButtonOverrides {
    pub color: Option<ColorSource>,
    pub text_color: Option<ColorSource>,
    pub border_color: Option<ColorSource>,
    pub border_widths: Option<Insets<f32>>,
    pub radius: Option<Corners<f32>>,
    pub hover: Option<NativeButtonStateStyle>,
    pub pressed: Option<NativeButtonStateStyle>,
    pub focused: Option<NativeButtonStateStyle>,
    pub disabled: Option<NativeButtonStateStyle>,
}

/// The internal styling rules and state layers for a button variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ButtonStyle {
    pub(crate) bg: Option<ColorRole>,
    pub(crate) fg: ColorRole,
    pub(crate) border: Option<ColorRole>,
    pub(crate) border_thickness: f32,

    pub(crate) hover_layer: StateLayer,
    pub(crate) focus_layer: StateLayer,
    pub(crate) pressed_layer: StateLayer,

    pub(crate) focus_border: Option<ColorRole>,
    pub(crate) focus_border_thickness: f32,

    pub(crate) disabled_bg: Option<ColorRole>,
    pub(crate) disabled_bg_opacity: f32,
    pub(crate) disabled_fg: ColorRole,
    pub(crate) disabled_fg_opacity: f32,
    pub(crate) disabled_border: Option<ColorRole>,
    pub(crate) disabled_border_opacity: f32,
}

impl ButtonStyle {
    pub(crate) fn filled() -> Self {
        Self {
            bg: Some(ColorRole::Primary),
            fg: ColorRole::OnPrimary,
            border: None,
            border_thickness: 0.0,
            hover_layer: StateLayer::new(ColorRole::OnPrimary, 0.08),
            focus_layer: StateLayer::new(ColorRole::OnPrimary, 0.10),
            pressed_layer: StateLayer::new(ColorRole::OnPrimary, 0.12),
            focus_border: None,
            focus_border_thickness: 2.0,
            disabled_bg: Some(ColorRole::OnSurface),
            disabled_bg_opacity: 0.12,
            disabled_fg: ColorRole::OnSurface,
            disabled_fg_opacity: 0.38,
            disabled_border: None,
            disabled_border_opacity: 0.0,
        }
    }

    pub(crate) fn tonal() -> Self {
        Self {
            bg: Some(ColorRole::SecondaryContainer),
            fg: ColorRole::OnSecondaryContainer,
            border: None,
            border_thickness: 0.0,
            hover_layer: StateLayer::new(ColorRole::OnSecondaryContainer, 0.08),
            focus_layer: StateLayer::new(ColorRole::OnSecondaryContainer, 0.10),
            pressed_layer: StateLayer::new(ColorRole::OnSecondaryContainer, 0.12),
            focus_border: None,
            focus_border_thickness: 2.0,
            disabled_bg: Some(ColorRole::OnSurface),
            disabled_bg_opacity: 0.12,
            disabled_fg: ColorRole::OnSurface,
            disabled_fg_opacity: 0.38,
            disabled_border: None,
            disabled_border_opacity: 0.0,
        }
    }

    pub(crate) fn outlined() -> Self {
        Self {
            bg: None,
            fg: ColorRole::Primary,
            border: Some(ColorRole::Outline),
            border_thickness: 1.0,
            hover_layer: StateLayer::new(ColorRole::Primary, 0.08),
            focus_layer: StateLayer::new(ColorRole::Primary, 0.10),
            pressed_layer: StateLayer::new(ColorRole::Primary, 0.12),
            focus_border: Some(ColorRole::Primary),
            focus_border_thickness: 2.0,
            disabled_bg: None,
            disabled_bg_opacity: 0.0,
            disabled_fg: ColorRole::OnSurface,
            disabled_fg_opacity: 0.38,
            disabled_border: Some(ColorRole::OnSurface),
            disabled_border_opacity: 0.12,
        }
    }

    pub(crate) fn text() -> Self {
        Self {
            bg: None,
            fg: ColorRole::Primary,
            border: None,
            border_thickness: 0.0,
            hover_layer: StateLayer::new(ColorRole::Primary, 0.08),
            focus_layer: StateLayer::new(ColorRole::Primary, 0.10),
            pressed_layer: StateLayer::new(ColorRole::Primary, 0.12),
            focus_border: Some(ColorRole::Outline),
            focus_border_thickness: 2.0,
            disabled_bg: None,
            disabled_bg_opacity: 0.0,
            disabled_fg: ColorRole::OnSurface,
            disabled_fg_opacity: 0.38,
            disabled_border: None,
            disabled_border_opacity: 0.0,
        }
    }

    pub(crate) fn for_variant(variant: ButtonVariant) -> Self {
        match variant {
            ButtonVariant::Filled => Self::filled(),
            ButtonVariant::Tonal => Self::tonal(),
            ButtonVariant::Outlined => Self::outlined(),
            ButtonVariant::Text => Self::text(),
        }
    }
}

/// Resolves button visual properties and foreground colours from theme tokens and overrides.
pub(crate) fn resolve_button_visuals(
    variant: ButtonVariant,
    size: ButtonSize,
    overrides: ButtonOverrides,
    reader: &impl ThemeReader,
) -> (NativeButtonProps, Color, Color) {
    let scheme = &reader.theme().colors;
    let style = ButtonStyle::for_variant(variant);

    let fade = |c: Color, a: f32| Color::rgba(c.r, c.g, c.b, c.a * a);

    let base_bg = overrides
        .color
        .map(|c| c.resolve(reader))
        .unwrap_or_else(|| {
            style
                .bg
                .map(|r| r.resolve(scheme))
                .unwrap_or(Color::TRANSPARENT)
        });

    let base_border_color = overrides
        .border_color
        .map(|c| c.resolve(reader))
        .unwrap_or_else(|| {
            style
                .border
                .map(|r| r.resolve(scheme))
                .unwrap_or(Color::TRANSPARENT)
        });

    let border_insets = overrides
        .border_widths
        .unwrap_or(Insets::all(style.border_thickness));

    let radius = overrides
        .radius
        .unwrap_or_else(|| Corners::all(size.shape().resolve_radius_dp(size.height())));

    let hover_bg = style.hover_layer.blend_over(base_bg, scheme);
    let pressed_bg = style.pressed_layer.blend_over(base_bg, scheme);
    let focus_bg = style.focus_layer.blend_over(base_bg, scheme);
    let focus_border_color = style
        .focus_border
        .unwrap_or(ColorRole::Outline)
        .resolve(scheme);
    let focus_border_insets = Insets::all(style.focus_border_thickness);

    let disabled_bg = style
        .disabled_bg
        .map(|r| fade(r.resolve(scheme), style.disabled_bg_opacity))
        .unwrap_or(Color::TRANSPARENT);
    let disabled_border_color = style
        .disabled_border
        .map(|r| fade(r.resolve(scheme), style.disabled_border_opacity))
        .unwrap_or(Color::TRANSPARENT);

    let theme_hover = NativeButtonStateStyle::default()
        .background(hover_bg)
        .border(border_insets, base_border_color);
    let theme_pressed = NativeButtonStateStyle::default()
        .background(pressed_bg)
        .border(border_insets, base_border_color);
    let theme_focused = NativeButtonStateStyle::default()
        .background(focus_bg)
        .border(focus_border_insets, focus_border_color);
    let theme_disabled = NativeButtonStateStyle::default()
        .background(disabled_bg)
        .border(border_insets, disabled_border_color);

    let hover = match overrides.hover {
        Some(ov) => NativeButtonStateStyle {
            background: ov.background.or(theme_hover.background),
            border_widths: ov.border_widths.or(theme_hover.border_widths),
            border_color: ov.border_color.or(theme_hover.border_color),
        },
        None => theme_hover,
    };

    let pressed = match overrides.pressed {
        Some(ov) => NativeButtonStateStyle {
            background: ov.background.or(theme_pressed.background),
            border_widths: ov.border_widths.or(theme_pressed.border_widths),
            border_color: ov.border_color.or(theme_pressed.border_color),
        },
        None => theme_pressed,
    };

    let focused = match overrides.focused {
        Some(ov) => NativeButtonStateStyle {
            background: ov.background.or(theme_focused.background),
            border_widths: ov.border_widths.or(theme_focused.border_widths),
            border_color: ov.border_color.or(theme_focused.border_color),
        },
        None => theme_focused,
    };

    let disabled = match overrides.disabled {
        Some(ov) => NativeButtonStateStyle {
            background: ov.background.or(theme_disabled.background),
            border_widths: ov.border_widths.or(theme_disabled.border_widths),
            border_color: ov.border_color.or(theme_disabled.border_color),
        },
        None => theme_disabled,
    };

    let fg = overrides
        .text_color
        .map(|c| c.resolve(reader))
        .unwrap_or_else(|| style.fg.resolve(scheme));
    let disabled_fg = fade(style.disabled_fg.resolve(scheme), style.disabled_fg_opacity);

    let props = NativeButtonProps {
        background: base_bg,
        radius,
        border_widths: border_insets,
        border_color: base_border_color,
        hover,
        pressed,
        focused,
        disabled,
        state: WidgetState::Enabled,
    };

    (props, fg, disabled_fg)
}

pub fn button() -> ButtonBuilder {
    ButtonBuilder::new()
}

/// Builder for a themed [`Button`].
#[derive(Debug, Clone)]
pub struct ButtonBuilder {
    pub props: NativeButtonProps,
    pub variant: ButtonVariant,
    pub size: ButtonSize,
    pub label: Option<String>,
    pub icon: Option<SpriteId>,
    pub overrides: ButtonOverrides,
    pub style: Option<LayoutStyle>,
    pub width: Option<Val>,
    pub height: Option<Val>,
    pub icon_gap: Option<Val>,
}

impl Deref for ButtonBuilder {
    type Target = NativeButtonProps;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.props
    }
}

impl Default for ButtonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ButtonBuilder {
    pub fn new() -> Self {
        Self {
            props: NativeButtonProps::default(),
            variant: ButtonVariant::Filled,
            size: ButtonSize::Medium,
            label: None,
            icon: None,
            overrides: ButtonOverrides::default(),
            style: None,
            width: None,
            height: None,
            icon_gap: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: SpriteId) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn state(mut self, state: WidgetState) -> Self {
        self.props.state = state;
        self
    }

    pub fn disabled(self, disabled: bool) -> Self {
        self.state(if disabled {
            WidgetState::Disabled
        } else {
            WidgetState::Enabled
        })
    }

    pub fn focused(self, focused: bool) -> Self {
        self.state(if focused {
            WidgetState::Focused
        } else {
            WidgetState::Enabled
        })
    }

    pub fn color(mut self, color: impl IntoColorSource) -> Self {
        self.overrides.color = color.into_color_source();
        self
    }

    pub fn text_color(mut self, color: impl IntoColorSource) -> Self {
        self.overrides.text_color = color.into_color_source();
        self
    }

    pub fn border_color(mut self, color: impl IntoColorSource) -> Self {
        self.overrides.border_color = color.into_color_source();
        self
    }

    pub fn border(
        mut self,
        widths: impl Into<Option<Insets<f32>>>,
        color: impl IntoColorSource,
    ) -> Self {
        self.overrides.border_widths = widths.into();
        self.overrides.border_color = color.into_color_source();
        self
    }

    pub fn radius(mut self, radius: impl Into<Option<Corners<f32>>>) -> Self {
        self.overrides.radius = radius.into();
        self
    }

    pub fn shape(mut self, shape: impl Into<Option<Shape>>) -> Self {
        self.overrides.radius = shape
            .into()
            .map(|s| Corners::all(s.resolve_radius_dp(self.size.height())));
        self
    }

    pub fn background(self, color: impl IntoColorSource) -> Self {
        self.color(color)
    }

    pub fn border_widths(mut self, widths: impl Into<Option<Insets<f32>>>) -> Self {
        self.overrides.border_widths = widths.into();
        self
    }

    pub fn hover_style(mut self, style: impl Into<Option<NativeButtonStateStyle>>) -> Self {
        self.overrides.hover = style.into();
        self
    }

    pub fn pressed_style(mut self, style: impl Into<Option<NativeButtonStateStyle>>) -> Self {
        self.overrides.pressed = style.into();
        self
    }

    pub fn focused_style(mut self, style: impl Into<Option<NativeButtonStateStyle>>) -> Self {
        self.overrides.focused = style.into();
        self
    }

    pub fn disabled_style(mut self, style: impl Into<Option<NativeButtonStateStyle>>) -> Self {
        self.overrides.disabled = style.into();
        self
    }

    pub fn hover_background(mut self, color: Color) -> Self {
        self.overrides
            .hover
            .get_or_insert_with(Default::default)
            .background = Some(color);
        self
    }

    pub fn hover_border(mut self, widths: Insets<f32>, color: Color) -> Self {
        let h = self.overrides.hover.get_or_insert_with(Default::default);
        h.border_widths = Some(widths);
        h.border_color = Some(color);
        self
    }

    pub fn pressed_background(mut self, color: Color) -> Self {
        self.overrides
            .pressed
            .get_or_insert_with(Default::default)
            .background = Some(color);
        self
    }

    pub fn pressed_border(mut self, widths: Insets<f32>, color: Color) -> Self {
        let p = self.overrides.pressed.get_or_insert_with(Default::default);
        p.border_widths = Some(widths);
        p.border_color = Some(color);
        self
    }

    pub fn focused_background(mut self, color: Color) -> Self {
        self.overrides
            .focused
            .get_or_insert_with(Default::default)
            .background = Some(color);
        self
    }

    pub fn focused_border(mut self, widths: Insets<f32>, color: Color) -> Self {
        let f = self.overrides.focused.get_or_insert_with(Default::default);
        f.border_widths = Some(widths);
        f.border_color = Some(color);
        self
    }

    pub fn disabled_background(mut self, color: Color) -> Self {
        self.overrides
            .disabled
            .get_or_insert_with(Default::default)
            .background = Some(color);
        self
    }

    pub fn disabled_border(mut self, widths: Insets<f32>, color: Color) -> Self {
        let d = self.overrides.disabled.get_or_insert_with(Default::default);
        d.border_widths = Some(widths);
        d.border_color = Some(color);
        self
    }

    pub fn style(mut self, style: LayoutStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn width(mut self, width: Val) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: Val) -> Self {
        self.height = Some(height);
        self
    }

    /// Space between the icon and the label. `None` uses the size's default gap.
    pub fn icon_gap(mut self, gap: impl Into<Option<Val>>) -> Self {
        self.icon_gap = gap.into();
        self
    }

    /// Resolves theme colors, state layers, and shapes for the button.
    pub fn resolve_props(&self, reader: &impl ThemeReader) -> (NativeButtonProps, Color, Color) {
        let (mut props, fg, disabled_fg) =
            resolve_button_visuals(self.variant, self.size, self.overrides, reader);
        props.state = self.props.state;
        (props, fg, disabled_fg)
    }
}

impl Build for ButtonBuilder {
    type Widget = Button;
}

/// Themed Mechanix Design Button widget.
#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub(crate) props: NativeButtonProps,
    pub(crate) variant: ButtonVariant,
    pub(crate) size: ButtonSize,
    pub(crate) overrides: ButtonOverrides,
    pub(crate) icon_gap: Option<Val>,
    pub(crate) fg: Color,
    pub(crate) native: Handle<NativeButton>,
    pub(crate) label_handle: Option<Handle<Text>>,
    pub(crate) icon_handle: Option<Handle<Icon>>,
}

impl Deref for Button {
    type Target = NativeButtonProps;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.props
    }
}

impl Button {
    #[inline]
    pub fn native(&self) -> Handle<NativeButton> {
        self.native
    }

    #[inline]
    pub fn variant(&self) -> ButtonVariant {
        self.variant
    }

    #[inline]
    pub fn size(&self) -> ButtonSize {
        self.size
    }

    #[inline]
    pub fn overrides(&self) -> &ButtonOverrides {
        &self.overrides
    }

    #[inline]
    pub fn icon_gap(&self) -> Option<Val> {
        self.icon_gap
    }

    #[inline]
    pub fn fg(&self) -> Color {
        self.fg
    }

    #[inline]
    pub fn label(&self) -> Option<Handle<Text>> {
        self.label_handle
    }

    #[inline]
    pub fn icon(&self) -> Option<Handle<Icon>> {
        self.icon_handle
    }
}

impl Widget for Button {
    type Builder = ButtonBuilder;

    fn build(b: ButtonBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let (native_props, fg, disabled_fg) = b.resolve_props(s);
        let effective_fg = if b.props.is_disabled() {
            disabled_fg
        } else {
            fg
        };

        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default();

        let mut button_style = b.style.unwrap_or_else(|| {
            LayoutStyle::default()
                .row()
                .center()
                .column_gap(b.size.gap())
                .padding(b.size.padding())
                .min_height(px(b.size.height()))
        });

        if let Some(w) = b.width {
            button_style = button_style.width(w);
        }
        if let Some(h) = b.height {
            button_style = button_style.height(h);
        }
        if let Some(g) = b.icon_gap {
            button_style = button_style.column_gap(g);
        }

        let native = s.spawn(
            me,
            NativeButtonBuilder::from(native_props).style(button_style),
        );

        let icon_handle = b.icon.map(|sprite| {
            let icon_px = px(b.size.icon_size());
            s.spawn(
                native,
                widgets::icon(sprite)
                    .color(effective_fg)
                    .style(LayoutStyle::default().size(icon_px, icon_px)),
            )
        });

        let label_handle = b.label.map(|txt| {
            s.spawn(
                native,
                text(txt).variant(b.size.text_variant()).color(effective_fg),
            )
        });

        s.on_theme(me, sync_visuals);

        Button {
            props: native_props,
            variant: b.variant,
            size: b.size,
            overrides: b.overrides,
            icon_gap: b.icon_gap,
            fg,
            native,
            label_handle,
            icon_handle,
        }
    }
}

/// Synchronizes underlying native button, text, and icon visuals with the active theme and state.
fn sync_visuals(ctx: &mut Context<'_, Button>) {
    let (variant, size, state, overrides, native, label_h, icon_h) = {
        let me = ctx.me();
        (
            me.variant,
            me.size,
            me.props.state,
            me.overrides,
            me.native,
            me.label_handle,
            me.icon_handle,
        )
    };

    let (mut props, fg, disabled_fg) = resolve_button_visuals(variant, size, overrides, ctx);
    props.state = state;
    let effective_fg = if state.is_disabled() { disabled_fg } else { fg };

    ctx.me().fg = fg;
    ctx.me().props = props;

    if let Some(mut native_ctx) = ctx.at(native) {
        native_ctx.set_background(props.background);
        native_ctx.set_radius(props.radius);
        native_ctx.set_border(props.border_widths, props.border_color);
        native_ctx.set_hover_style(props.hover);
        native_ctx.set_pressed_style(props.pressed);
        native_ctx.set_focused_style(props.focused);
        native_ctx.set_disabled_style(props.disabled);
        native_ctx.set_state(state);
    }

    if let Some(lbl) = label_h
        && let Some(mut lbl_ctx) = ctx.at(lbl)
    {
        lbl_ctx.set_variant(size.text_variant());
        lbl_ctx.set_color(effective_fg);
    }

    if let Some(icn) = icon_h
        && let Some(mut icn_ctx) = ctx.at(icn)
    {
        icn_ctx.set_color(effective_fg);
    }
}

/// Extension trait for mutating a spawned [`Button`].
pub trait ButtonContextExt {
    fn set_label(&mut self, label: impl Into<String>);
    fn set_icon(&mut self, icon: SpriteId);
    fn set_variant(&mut self, variant: ButtonVariant);
    fn set_size(&mut self, size: ButtonSize);
    fn set_state(&mut self, state: WidgetState);
    fn set_disabled(&mut self, disabled: bool);
    fn set_focused(&mut self, focused: bool);

    fn set_color(&mut self, color: impl IntoColorSource);
    fn set_background(&mut self, color: impl IntoColorSource);
    fn set_text_color(&mut self, color: impl IntoColorSource);
    fn set_border_color(&mut self, color: impl IntoColorSource);
    fn set_border(&mut self, widths: impl Into<Option<Insets<f32>>>, color: impl IntoColorSource);
    fn set_border_widths(&mut self, widths: impl Into<Option<Insets<f32>>>);
    fn set_radius(&mut self, radius: impl Into<Option<Corners<f32>>>);
    fn set_shape(&mut self, shape: impl Into<Option<Shape>>);
    fn set_icon_gap(&mut self, gap: impl Into<Option<Val>>);

    fn set_hover_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>);
    fn set_pressed_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>);
    fn set_focused_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>);
    fn set_disabled_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>);

    fn sync_visuals(&mut self);
}

impl ButtonContextExt for Context<'_, Button> {
    fn set_label(&mut self, label: impl Into<String>) {
        if let Some(lbl) = self.me().label_handle
            && let Some(mut c) = self.at(lbl)
        {
            c.set_text(label);
        }
    }

    fn set_icon(&mut self, icon: SpriteId) {
        if let Some(icn) = self.me().icon_handle
            && let Some(mut c) = self.at(icn)
        {
            c.set_sprite(icon);
        }
    }

    fn set_variant(&mut self, variant: ButtonVariant) {
        if self.me().variant == variant {
            return;
        }
        self.me().variant = variant;
        sync_visuals(self);
    }

    fn set_size(&mut self, size: ButtonSize) {
        if self.me().size == size {
            return;
        }
        self.me().size = size;
        sync_visuals(self);
    }

    fn set_state(&mut self, state: WidgetState) {
        if self.me().props.state == state {
            return;
        }
        self.me().props.state = state;
        sync_visuals(self);
    }

    fn set_disabled(&mut self, disabled: bool) {
        let target = if disabled {
            WidgetState::Disabled
        } else {
            WidgetState::Enabled
        };
        self.set_state(target);
    }

    fn set_focused(&mut self, focused: bool) {
        let target = if focused {
            WidgetState::Focused
        } else {
            WidgetState::Enabled
        };
        self.set_state(target);
    }

    fn set_color(&mut self, color: impl IntoColorSource) {
        let cs = color.into_color_source();
        if self.me().overrides.color == cs {
            return;
        }
        self.me().overrides.color = cs;
        sync_visuals(self);
    }

    fn set_background(&mut self, color: impl IntoColorSource) {
        self.set_color(color);
    }

    fn set_text_color(&mut self, color: impl IntoColorSource) {
        let cs = color.into_color_source();
        if self.me().overrides.text_color == cs {
            return;
        }
        self.me().overrides.text_color = cs;
        sync_visuals(self);
    }

    fn set_border_color(&mut self, color: impl IntoColorSource) {
        let cs = color.into_color_source();
        if self.me().overrides.border_color == cs {
            return;
        }
        self.me().overrides.border_color = cs;
        sync_visuals(self);
    }

    fn set_border(&mut self, widths: impl Into<Option<Insets<f32>>>, color: impl IntoColorSource) {
        let w = widths.into();
        let cs = color.into_color_source();
        if self.me().overrides.border_widths == w && self.me().overrides.border_color == cs {
            return;
        }
        self.me().overrides.border_widths = w;
        self.me().overrides.border_color = cs;
        sync_visuals(self);
    }

    fn set_border_widths(&mut self, widths: impl Into<Option<Insets<f32>>>) {
        let w = widths.into();
        if self.me().overrides.border_widths == w {
            return;
        }
        self.me().overrides.border_widths = w;
        sync_visuals(self);
    }

    fn set_radius(&mut self, radius: impl Into<Option<Corners<f32>>>) {
        let r = radius.into();
        if self.me().overrides.radius == r {
            return;
        }
        self.me().overrides.radius = r;
        sync_visuals(self);
    }

    fn set_shape(&mut self, shape: impl Into<Option<Shape>>) {
        let r = shape
            .into()
            .map(|s| Corners::all(s.resolve_radius_dp(self.me().size.height())));
        if self.me().overrides.radius == r {
            return;
        }
        self.me().overrides.radius = r;
        sync_visuals(self);
    }

    fn set_icon_gap(&mut self, gap: impl Into<Option<Val>>) {
        let g = gap.into();
        if self.me().icon_gap == g {
            return;
        }
        self.me().icon_gap = g;
        let (native, size) = (self.me().native, self.me().size);
        if let Some(mut native_ctx) = self.at(native) {
            native_ctx.set_column_gap(g.unwrap_or(size.gap()));
        }
    }

    fn set_hover_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>) {
        let s = style.into();
        if self.me().overrides.hover == s {
            return;
        }
        self.me().overrides.hover = s;
        sync_visuals(self);
    }

    fn set_pressed_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>) {
        let s = style.into();
        if self.me().overrides.pressed == s {
            return;
        }
        self.me().overrides.pressed = s;
        sync_visuals(self);
    }

    fn set_focused_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>) {
        let s = style.into();
        if self.me().overrides.focused == s {
            return;
        }
        self.me().overrides.focused = s;
        sync_visuals(self);
    }

    fn set_disabled_style(&mut self, style: impl Into<Option<NativeButtonStateStyle>>) {
        let s = style.into();
        if self.me().overrides.disabled == s {
            return;
        }
        self.me().overrides.disabled = s;
        sync_visuals(self);
    }

    fn sync_visuals(&mut self) {
        sync_visuals(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_styles() {
        let filled = ButtonStyle::for_variant(ButtonVariant::Filled);
        assert_eq!(filled.bg, Some(ColorRole::Primary));
        assert_eq!(filled.fg, ColorRole::OnPrimary);

        let tonal = ButtonStyle::for_variant(ButtonVariant::Tonal);
        assert_eq!(tonal.bg, Some(ColorRole::SecondaryContainer));
        assert_eq!(tonal.fg, ColorRole::OnSecondaryContainer);

        let outlined = ButtonStyle::for_variant(ButtonVariant::Outlined);
        assert_eq!(outlined.bg, None);
        assert_eq!(outlined.fg, ColorRole::Primary);
        assert_eq!(outlined.border, Some(ColorRole::Outline));

        let text_style = ButtonStyle::for_variant(ButtonVariant::Text);
        assert_eq!(text_style.bg, None);
        assert_eq!(text_style.fg, ColorRole::Primary);
        assert_eq!(text_style.border, None);
    }

    #[test]
    fn size_properties() {
        assert_eq!(ButtonSize::XSmall.height(), 24.0);
        assert_eq!(ButtonSize::Small.height(), 32.0);
        assert_eq!(ButtonSize::Medium.height(), 40.0);
        assert_eq!(ButtonSize::Large.height(), 48.0);
        assert_eq!(ButtonSize::XLarge.height(), 56.0);

        assert_eq!(ButtonSize::XSmall.shape(), Shape::ExtraSmall);
        assert_eq!(ButtonSize::Medium.shape(), Shape::Small);
        assert_eq!(ButtonSize::XLarge.shape(), Shape::Medium);
    }

    #[test]
    fn variant_fg_roles() {
        assert_eq!(ButtonVariant::Filled.fg_role(), ColorRole::OnPrimary);
        assert_eq!(
            ButtonVariant::Tonal.fg_role(),
            ColorRole::OnSecondaryContainer
        );
        assert_eq!(ButtonVariant::Outlined.fg_role(), ColorRole::Primary);
        assert_eq!(ButtonVariant::Text.fg_role(), ColorRole::Primary);
    }

    #[test]
    fn builder_defaults() {
        let b = button();
        assert_eq!(b.variant, ButtonVariant::Filled);
        assert_eq!(b.size, ButtonSize::Medium);
        assert_eq!(b.label, None);
        assert_eq!(b.icon, None);
        assert_eq!(b.state, WidgetState::Enabled);
        assert_eq!(b.overrides, ButtonOverrides::default());
    }

    #[test]
    fn builder_overrides_and_props_deref() {
        let b = button()
            .variant(ButtonVariant::Tonal)
            .size(ButtonSize::Small)
            .label("Submit")
            .icon(SpriteId(42))
            .disabled(true)
            .radius(Corners::all(8.0));

        assert_eq!(b.variant, ButtonVariant::Tonal);
        assert_eq!(b.size, ButtonSize::Small);
        assert_eq!(b.label, Some("Submit".to_string()));
        assert_eq!(b.icon, Some(SpriteId(42)));
        assert_eq!(b.state, WidgetState::Disabled);
        assert_eq!(b.overrides.radius, Some(Corners::all(8.0)));
        assert!(b.is_disabled());

        let b2 = button().border(Insets::all(2.0), ColorRole::Error);
        assert_eq!(b2.overrides.border_widths, Some(Insets::all(2.0)));
        assert!(b2.overrides.border_color.is_some());
    }

    #[test]
    fn button_widget_spawns_and_syncs_on_update_and_theme() {
        use crate::NativeText;
        use app::App;
        use atlas::Atlas;
        use layout::LayoutModule;
        use paint::PaintModule;
        use theme::prelude::*;

        let mut app = App::new();
        app.add_module(MechanixTheme::dark());
        app.add_module(LayoutModule);
        app.add_module(PaintModule);

        let mut atlas = Atlas::new();
        let font_id = atlas
            .add_font(include_bytes!(
                "../../../crates/atlas/tests/fixtures/Inter-Regular.ttf"
            ))
            .unwrap();
        app.insert_resource(atlas);
        app.insert_resource(crate::font::FontBook::new(font_id));

        let btn_h = app.spawn(app.root(), button().label("Click Me"));
        app.tick();

        let btn = app.widget::<Button>(btn_h).unwrap();
        let native_h = btn.native();
        let label_h = btn.label().expect("label handle present");

        let native_btn = app.widget::<NativeButton>(native_h).unwrap();
        assert_eq!(native_btn.state, WidgetState::Enabled);
        let dark_bg = native_btn.background;
        assert_eq!(dark_bg, app.color(ColorRole::Primary));

        let text_w = app
            .widget::<NativeText>(app.widget::<Text>(label_h).unwrap().native())
            .unwrap();
        assert_eq!(text_w.string, "Click Me");
        assert_eq!(text_w.color, app.color(ColorRole::OnPrimary));

        // Mutate label and variant via context
        struct Poke;
        impl app::Event for Poke {}

        struct Controller;
        struct ControllerBuilder(Box<dyn FnMut(&mut Context<'_, Controller>)>);
        impl Build for ControllerBuilder {
            type Widget = Controller;
        }
        impl Widget for Controller {
            type Builder = ControllerBuilder;
            fn build(b: ControllerBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
                let mut action = b.0;
                s.on::<Poke>(me, move |ctx, _| action(ctx));
                Controller
            }
        }

        let controller = app.spawn(
            app.root(),
            ControllerBuilder(Box::new(move |ctx: &mut Context<'_, Controller>| {
                let mut c = ctx.at(btn_h).unwrap();
                c.set_label("Updated Label");
                c.set_variant(ButtonVariant::Tonal);
            })),
        );

        app.emit(Poke, controller.id());
        app.flush();
        app.tick();

        let native_btn = app.widget::<NativeButton>(native_h).unwrap();
        assert_eq!(
            native_btn.background,
            app.color(ColorRole::SecondaryContainer)
        );

        let text_w = app
            .widget::<NativeText>(app.widget::<Text>(label_h).unwrap().native())
            .unwrap();
        assert_eq!(text_w.string, "Updated Label");
        assert_eq!(text_w.color, app.color(ColorRole::OnSecondaryContainer));

        // Switch theme to light
        app.set_theme(MechanixTheme::light());
        app.tick();

        let native_btn = app.widget::<NativeButton>(native_h).unwrap();
        let light_bg = native_btn.background;
        assert_eq!(light_bg, app.color(ColorRole::SecondaryContainer));
        assert_ne!(light_bg, dark_bg);
    }

    #[test]
    fn button_setters_accept_values_and_none_to_clear() {
        use app::App;
        use atlas::Atlas;
        use layout::LayoutModule;
        use paint::PaintModule;
        use theme::prelude::*;

        let mut app = App::new();
        app.add_module(MechanixTheme::dark());
        app.add_module(LayoutModule);
        app.add_module(PaintModule);

        let mut atlas = Atlas::new();
        let font_id = atlas
            .add_font(include_bytes!(
                "../../../crates/atlas/tests/fixtures/Inter-Regular.ttf"
            ))
            .unwrap();
        app.insert_resource(atlas);
        app.insert_resource(crate::font::FontBook::new(font_id));

        let btn_h = app.spawn(app.root(), button().label("Test"));
        app.tick();

        struct Poke;
        impl app::Event for Poke {}

        struct Controller;
        struct ControllerBuilder(Box<dyn FnMut(&mut Context<'_, Controller>)>);
        impl Build for ControllerBuilder {
            type Widget = Controller;
        }
        impl Widget for Controller {
            type Builder = ControllerBuilder;
            fn build(b: ControllerBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
                let mut action = b.0;
                s.on::<Poke>(me, move |ctx, _| action(ctx));
                Controller
            }
        }

        // Set explicit color, radius, and border
        let controller1 = app.spawn(
            app.root(),
            ControllerBuilder(Box::new(move |ctx: &mut Context<'_, Controller>| {
                let mut c = ctx.at(btn_h).unwrap();
                c.set_color(ColorRole::Error);
                c.set_radius(Corners::all(16.0));
                c.set_border(Insets::all(2.0), ColorRole::Primary);
            })),
        );
        app.emit(Poke, controller1.id());
        app.flush();
        app.tick();

        let btn = app.widget::<Button>(btn_h).unwrap();
        assert_eq!(
            btn.overrides.color,
            Some(ColorSource::Role(ColorRole::Error))
        );
        assert_eq!(btn.overrides.radius, Some(Corners::all(16.0)));
        assert_eq!(btn.overrides.border_widths, Some(Insets::all(2.0)));
        assert_eq!(
            btn.overrides.border_color,
            Some(ColorSource::Role(ColorRole::Primary))
        );

        // Now clear them using None instead of clear_* methods!
        let controller2 = app.spawn(
            app.root(),
            ControllerBuilder(Box::new(move |ctx: &mut Context<'_, Controller>| {
                let mut c = ctx.at(btn_h).unwrap();
                c.set_color(None);
                c.set_radius(None);
                c.set_border(None, None);
            })),
        );
        app.emit(Poke, controller2.id());
        app.flush();
        app.tick();

        let btn = app.widget::<Button>(btn_h).unwrap();
        assert_eq!(btn.overrides.color, None);
        assert_eq!(btn.overrides.radius, None);
        assert_eq!(btn.overrides.border_widths, None);
        assert_eq!(btn.overrides.border_color, None);
    }

    #[test]
    fn state_style_overrides_and_builder_setters() {
        let green = Color::rgba(0.0, 1.0, 0.0, 1.0);
        let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
        let b = button()
            .background(Color::rgba(0.1, 0.2, 0.3, 1.0))
            .border_widths(Insets::all(3.0))
            .hover_background(green)
            .hover_border(Insets::all(4.0), Color::WHITE)
            .pressed_background(blue);

        assert_eq!(
            b.overrides.color,
            Some(ColorSource::Exact(Color::rgba(0.1, 0.2, 0.3, 1.0)))
        );
        assert_eq!(b.overrides.border_widths, Some(Insets::all(3.0)));
        let hover = b.overrides.hover.unwrap();
        assert_eq!(hover.background, Some(green));
        assert_eq!(hover.border_widths, Some(Insets::all(4.0)));
        assert_eq!(hover.border_color, Some(Color::WHITE));

        let pressed = b.overrides.pressed.unwrap();
        assert_eq!(pressed.background, Some(blue));
        assert_eq!(pressed.border_widths, None);
    }

    #[test]
    fn builder_width_and_height() {
        let b = button().width(px(100.0)).height(px(40.0));
        assert_eq!(b.width, Some(px(100.0)));
        assert_eq!(b.height, Some(px(40.0)));
    }

    #[test]
    fn builder_icon_gap() {
        assert_eq!(button().icon_gap, None);
        let b = button().icon_gap(px(12.0));
        assert_eq!(b.icon_gap, Some(px(12.0)));
        assert_eq!(b.icon_gap(None).icon_gap, None);
    }

    #[test]
    fn context_focused_and_icon() {
        use app::App;
        use atlas::Atlas;
        use layout::LayoutModule;
        use paint::PaintModule;
        use theme::prelude::*;

        let mut app = App::new();
        app.add_module(MechanixTheme::dark());
        app.add_module(LayoutModule);
        app.add_module(PaintModule);

        let mut atlas = Atlas::new();
        let font_id = atlas
            .add_font(include_bytes!(
                "../../../crates/atlas/tests/fixtures/Inter-Regular.ttf"
            ))
            .unwrap();
        let sprite1 = atlas
            .insert(
                atlas::Class::Icon,
                &atlas::Bitmap {
                    width: 8,
                    height: 8,
                    format: atlas::Format::R8,
                    pixels: vec![255; 64],
                },
            )
            .unwrap();
        let sprite2 = atlas
            .insert(
                atlas::Class::Icon,
                &atlas::Bitmap {
                    width: 8,
                    height: 8,
                    format: atlas::Format::R8,
                    pixels: vec![255; 64],
                },
            )
            .unwrap();
        app.insert_resource(atlas);
        app.insert_resource(crate::font::FontBook::new(font_id));

        struct Poke;
        impl app::Event for Poke {}

        struct Controller;
        struct ControllerBuilder(Box<dyn FnMut(&mut Context<'_, Controller>)>);
        impl Build for ControllerBuilder {
            type Widget = Controller;
        }
        impl Widget for Controller {
            type Builder = ControllerBuilder;
            fn build(b: ControllerBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
                let mut action = b.0;
                s.on::<Poke>(me, move |ctx, _| action(ctx));
                Controller
            }
        }

        let btn_h = app.spawn(app.root(), button().label("Test").icon(sprite1));
        app.tick();

        let controller = app.spawn(
            app.root(),
            ControllerBuilder(Box::new(move |ctx: &mut Context<'_, Controller>| {
                let mut c = ctx.at(btn_h).unwrap();
                c.set_focused(true);
                c.set_icon(sprite2);
            })),
        );
        app.emit(Poke, controller.id());
        app.flush();
        app.tick();

        let btn = app.widget::<Button>(btn_h).unwrap();
        assert_eq!(btn.props.state, WidgetState::Focused);
    }

    #[test]
    fn icon_and_label_are_centered_on_both_axes() {
        use app::App;
        use atlas::Atlas;
        use layout::{Align, ComputedLayout, LayoutModule, LayoutRoot};
        use paint::PaintModule;
        use theme::prelude::*;

        let mut app = App::new();
        app.add_module(MechanixTheme::dark());
        app.add_module(LayoutModule);
        app.add_module(PaintModule);

        let mut atlas = Atlas::new();
        let font_id = atlas
            .add_font(include_bytes!(
                "../../../crates/atlas/tests/fixtures/Inter-Regular.ttf"
            ))
            .unwrap();
        let sprite = atlas
            .insert(
                atlas::Class::Icon,
                &atlas::Bitmap {
                    width: 8,
                    height: 8,
                    format: atlas::Format::R8,
                    pixels: vec![255; 64],
                },
            )
            .unwrap();
        app.insert_resource(atlas);
        app.insert_resource(crate::font::FontBook::new(font_id));

        struct Window;
        impl Build for Window {
            type Widget = Window;
        }
        impl Widget for Window {
            type Builder = Window;
            fn build(b: Window, _: Handle<Self>, _: &mut Spawner<'_, Self>) -> Self {
                b
            }
        }

        let window = app.spawn_with(
            app.root(),
            Window,
            (
                LayoutRoot(true),
                LayoutStyle::default()
                    .size(px(800.0), px(600.0))
                    .align_items(Align::Start),
            ),
        );
        let btn_h = app.spawn(
            window,
            button()
                .label("Save")
                .icon(sprite)
                .width(px(200.0))
                .height(px(80.0)),
        );
        app.tick();

        let btn = app.widget::<Button>(btn_h).unwrap();
        let rect = |id: app::NodeId| app.component::<ComputedLayout>(id).unwrap().rect;
        let outer = rect(btn.native().id());
        let icon = rect(btn.icon().unwrap().id());
        let label = rect(btn.label().unwrap().id());

        let mid_y = |r: geometry::Rect| r.origin.y + r.size.height / 2.0;
        let outer_mid_x = outer.origin.x + outer.size.width / 2.0;
        let content_mid_x = (icon.origin.x + label.origin.x + label.size.width) / 2.0;

        assert!((mid_y(icon) - mid_y(outer)).abs() <= 1.0);
        assert!((mid_y(label) - mid_y(outer)).abs() <= 1.0);
        assert!((content_mid_x - outer_mid_x).abs() <= 1.0);
    }
}
