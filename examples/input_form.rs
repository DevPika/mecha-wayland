//! A small form with a few labeled text inputs and a submit button,
//! showing the `Input` widget's configurable builder API in action.
//! Run under a Wayland session: `cargo run --example input_form`.

use mecha_wayland::prelude::*;

/// The whole form: title, three labeled inputs, and a submit button that prints
/// the entered text to stdout when clicked.
struct Form;

fn form(font: FontId) -> FormBuilder {
    FormBuilder { font }
}

struct FormBuilder {
    font: FontId,
}

impl Build for FormBuilder {
    type Widget = Form;
}

impl Widget for Form {
    type Builder = FormBuilder;
    fn build(b: FormBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let label_color = Color::rgb(0.8, 0.8, 0.82);
        let field_color = Color::rgb(0.92, 0.92, 0.95);
        let field_bg = Color::rgb(0.18, 0.18, 0.2);
        let field_border = Color::rgb(0.4, 0.4, 0.45);
        let accent = Color::rgb(0.25, 0.5, 0.9);

        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .column()
            .gap(px(10.0))
            .fill()
            .padding_all(px(16.0));

        s.spawn(
            me,
            text(b.font, "Sign In")
                .size(22)
                .color(field_color)
                .style(LayoutStyle::default().center()),
        );

        // Each row is a div holding a fixed-width label and a fill-width
        // input. Spawning the input here (rather than inside a child
        // widget) keeps its `Handle<Input>` in hand for the submit handler.
        let name_row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        s.spawn(
            name_row,
            text(b.font, "Name")
                .size(16)
                .color(label_color)
                .style(LayoutStyle::default().min_width(px(72.0))),
        );
        // `.text(..)` seeds initial content; the caret defaults to the
        // text colour set by `.color(..)`.
        let name = s.spawn(
            name_row,
            input(b.font)
                .size(18)
                .text("Ada")
                .style(LayoutStyle::default().fill())
                .border(1.0, field_border)
                .background(field_bg)
                .color(field_color),
        );

        let email_row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        s.spawn(
            email_row,
            text(b.font, "Email")
                .size(16)
                .color(label_color)
                .style(LayoutStyle::default().min_width(px(72.0))),
        );
        // `.caret(..)` overrides the caret colour independent of `.color`.
        // `.content_purpose(..)` tells text-input-v3 to raise an email
        // keyboard (or falls back to a plain keyboard without it).
        let email = s.spawn(
            email_row,
            input(b.font)
                .size(18)
                .style(LayoutStyle::default().fill().min_width(px(72.0)))
                .border(1.0, field_border)
                .background(field_bg)
                .color(field_color)
                .caret(accent)
                .content_purpose(ContentPurpose::Email),
        );

        let pw_row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        s.spawn(
            pw_row,
            text(b.font, "Password")
                .size(16)
                .color(label_color)
                .style(LayoutStyle::default().min_width(px(72.0))),
        );
        // A thicker accent border with no background fill.
        // `.content_purpose(..)` + `.content_hint(..)` mark this as a
        // password field: text-input-v3 gets `Password` purpose with
        // `SENSITIVEDATA | HIDDENTEXT` hints (and without text-input-v3
        // the widget is driven by plain keyboard events).
        let password = s.spawn(
            pw_row,
            input(b.font)
                .size(18)
                .style(LayoutStyle::default().fill().min_width(px(72.0)))
                .border(2.0, accent)
                .color(field_color)
                .content_purpose(ContentPurpose::Password)
                .content_hint(ContentHint::SENSITIVEDATA | ContentHint::HIDDENTEXT),
        );

        let submit = s.spawn(me, button(b.font, "Submit"));

        s.on::<Clicked>(submit, move |ctx, _| {
            let name = ctx.at(name).unwrap().me().text().to_string();
            let email = ctx.at(email).unwrap().me().text().to_string();
            let password = ctx.at(password).unwrap().me().text().to_string();
            println!("submit: name={name:?} email={email:?} password={password:?}");
        });

        Form
    }
}

/// A clickable box with a text label, lifted from `counter.rs`.
struct Button;

fn button(font: FontId, label: impl Into<String>) -> ButtonBuilder {
    ButtonBuilder {
        font,
        label: label.into(),
    }
}

struct ButtonBuilder {
    font: FontId,
    label: String,
}

impl Build for ButtonBuilder {
    type Widget = Button;
}

impl Widget for Button {
    type Builder = ButtonBuilder;
    fn build(b: ButtonBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() =
            LayoutStyle::default().center().padding_all(px(12.0));
        *s.component_mut::<Paint>(me).unwrap() =
            Paint::Quad(Quad::new(Color::rgb(0.25, 0.5, 0.9)).radius(6.0));
        s.spawn(me, text(b.font, b.label).size(18));
        Button
    }
}

/// Spawns the window and the form; stops the app on close.
struct Shell;
struct ShellBuilder {
    root: NodeId,
    font: FontId,
}
impl Build for ShellBuilder {
    type Widget = Shell;
}
impl Widget for Shell {
    type Builder = ShellBuilder;
    fn build(b: ShellBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let win = s.spawn(
            b.root,
            window()
                .title("input form")
                .clear(Color::rgb(0.12, 0.12, 0.14))
                .layout(LayoutStyle::default().center().size(px(360.0), px(280.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(win, form(b.font));
        Shell
    }
}

fn main() {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(InteractivityModule)
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());
    app.add_module(RingModule::default())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.input_form".into(),
            budget: Budget::default(),
        })
        .add_module(TextInputModule);

    let font = app
        .resource_mut::<Atlas>()
        .add_font(include_bytes!(
            "../crates/atlas/tests/fixtures/Inter-Regular.ttf"
        ))
        .expect("Inter loads");

    let root = app.root();
    app.spawn(root, ShellBuilder { root, font });
    app.run();
}
