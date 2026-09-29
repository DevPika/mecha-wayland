use std::time::Duration;

use mecha_wayland::prelude::*;

struct Leaf;
impl Build for Leaf {
	type Widget = Leaf;
}
impl Widget for Leaf {
	type Builder = Leaf;
	fn build(b: Leaf, _me: Handle<Self>, _s: &mut Spawner<'_, Self>) -> Self {
		b
	}
}

/// Spawns the window and its content; stops the app on close.
struct Shell;
struct ShellBuilder {
	root: NodeId,
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
				.title("showcase")
				.clear(Color::rgb(0.12, 0.12, 0.14))
				.layout(LayoutStyle::default().column().size(px(480.0), px(320.0))),
		);
		s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));

		// An opaque panel.
		let panel = s.spawn_with(
			win,
			Leaf,
			(
				LayoutStyle::default().column().size(px(440.0), px(200.0)),
				Paint::Quad(Quad::new(Color::rgb(0.2, 0.45, 0.8)).radius(12.0)),
			),
		);
		s.on::<Clicked>(panel, |_,e|{
			eprintln!("panel: Click {:?} at {:?}", e.contact, e.position);
		});

		let panel2 = s.spawn_with(
			win,
			Leaf,
			(
				AnimationSettings::new(
					AnimationTime::Duration(Duration::from_millis(1500)),
					|t| t * t * t,
				),
				LayoutStyle::default().column().size(px(440.0), px(200.0)),
				Paint::Quad(Quad::new(Color::rgb(1.0, 0.45, 0.8)).radius(12.0)),
			)
		);

		s.on::<Clicked>(panel2, move |ctx, _| {
			let mut panel = ctx.at(panel2).unwrap();
			let width = if panel.style().width == px(440.0) {
				px(220.0)
			} else {
				px(440.0)
			};
			panel.set_width(width);
		});

		let panel3 = s.spawn_with(
			win,
			Leaf,
			(
				AnimationSettings::new(
					AnimationTime::Duration(Duration::from_millis(1500)),
					|t| t, // linear easing
				),
				LayoutStyle::default().column().size(px(200.0), px(180.0)),
				Paint::Quad(Quad::new(Color::rgb(0.456, 0.30, 0.08)).radius(12.0)),
			)
		);

		s.on::<Clicked>(panel3, move |ctx, _| {
			let mut panel = ctx.at(panel3).unwrap();
			// change 
			*panel.component_mut::<AnimationSettings>().unwrap() =
			AnimationSettings::new(
				AnimationTime::Duration(Duration::from_millis(3000)),
				|t| t * t * t,
			);
			let width = if panel.style().width == px(440.0) {
				px(180.0)
			} else {
				px(440.0)
			};
			panel.set_width(width);
		});

		Shell
	}
}

fn main() {
	let mut app = App::new();
	app.add_module(LayoutModule)
		.add_module(PaintModule)
		.add_module(WindowModule)
		.add_module(InteractivityModule)
		.add_module(AnimationModule)
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
			app_id: "mecha.showcase".into(),
			budget: Budget::default(),
		});
	let root = app.root();
	app.spawn(root, ShellBuilder { root });
	app.run();
}
