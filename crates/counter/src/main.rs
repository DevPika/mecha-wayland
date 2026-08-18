#![recursion_limit = "2048"]
pub mod atlas {
    include!(concat!(env!("OUT_DIR"), "/counter_gen.rs"));
}

use app::prelude::*;
use io_ring::Ring;
use renderer::commands::Color;
use taffy::prelude::*;
use taffy::{Size, Style};
use ui::widgets::{BorderColor, DebugDamage, Div, Text};
use ui::{Damage, OnChange, Point, Render, RenderCommand, WidgetList};
use utils::Rect;
use wayland::{WlPointerButtonState, WlPointerEvent};
use window_manager::prelude::*;

const BG: Color = Color::from_rgb8(24, 24, 32);
const IDLE: Color = Color::from_rgb8(46, 52, 72);
const HOT: Color = Color::from_rgb8(74, 108, 170);
const BORDER: Color = Color::from_rgb8(120, 140, 190);
const LABEL: Color = Color::from_rgb8(220, 226, 240);
const COUNT: Color = Color::from_rgb8(240, 244, 255);

const BTN: f32 = 96.0;
const BTN_LEFT: u32 = 0x110;

const FONT: &assets::BakedFont = &atlas::COUNTER_FONT_INTER_64;

#[derive(Clone, Copy, PartialEq)]
enum Btn {
    Dec,
    Inc,
}

#[derive(Clone, Copy)]
struct Hover(Option<Point>);
#[derive(Clone, Copy)]
struct Click(Point);

type Button = Div<(Text,)>;

fn row_style() -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        justify_content: Some(JustifyContent::Center),
        align_items: Some(AlignItems::Center),
        size: Size {
            width: percent(1.0_f32),
            height: percent(1.0_f32),
        },
        gap: Size {
            width: length(28.0_f32),
            height: zero(),
        },
        ..Style::default()
    }
}

fn button_style() -> Style {
    Style {
        display: Display::Flex,
        justify_content: Some(JustifyContent::Center),
        align_items: Some(AlignItems::Center),
        size: Size {
            width: length(BTN),
            height: length(BTN),
        },
        ..Style::default()
    }
}

fn glyph(s: &str, color: Color) -> Text {
    let mut t = Text::new(Style::default());
    t.font = Some(FONT);
    t.text = s.into();
    t.color = color;
    t
}

fn button(label: &str) -> Button {
    let mut d = Div::new(button_style(), (glyph(label, LABEL),));
    d.color = IDLE;
    d.border_color = BorderColor(BORDER);
    d.border_thickness = 2.0;
    d.border_radius = 16.0;
    d
}

#[ui::widget]
struct Counter {
    count: u32,
    hovered: Option<Btn>,
    #[widget(child)]
    children: (Button, Text, Button),
}

impl Render for Counter {
    fn render(&self, _layout: &taffy::Layout, _abs_pos: Point) -> Vec<RenderCommand> {
        Vec::new()
    }
}

impl Counter {
    fn new() -> Self {
        Self {
            node_id: taffy::NodeId::new(u64::MAX),
            style: row_style(),
            bounds: Rect::ZERO,
            pending_damage: Damage::None,
            count: 0,
            hovered: None,
            children: (button("-"), glyph("0", COUNT), button("+")),
        }
    }

    fn hit(&self, p: Point) -> Option<Btn> {
        if self.children.0.bounds().contains_point(p) {
            Some(Btn::Dec)
        } else if self.children.2.bounds().contains_point(p) {
            Some(Btn::Inc)
        } else {
            None
        }
    }

    fn button_mut(&mut self, b: Btn) -> &mut Button {
        match b {
            Btn::Dec => &mut self.children.0,
            Btn::Inc => &mut self.children.2,
        }
    }
}

impl OnChange<Hover> for Counter {
    fn damage(&self, _new: &Hover) -> Damage {
        Damage::None
    }

    fn change(&mut self, new: Hover) {
        let target = new.0.and_then(|p| self.hit(p));
        if target == self.hovered {
            return;
        }
        if let Some(old) = self.hovered {
            self.button_mut(old).set(IDLE);
        }
        if let Some(t) = target {
            self.button_mut(t).set::<Color>(HOT);
        }
        self.hovered = target;
    }
}

impl OnChange<Click> for Counter {
    fn damage(&self, _new: &Click) -> Damage {
        Damage::None
    }

    fn change(&mut self, new: Click) {
        let Some(t) = self.hit(new.0) else {
            return;
        };
        match t {
            Btn::Inc => self.count += 1,
            Btn::Dec => self.count = self.count.saturating_sub(1),
        }
        self.children.1.set(self.count.to_string());
    }
}

#[derive(State)]
struct CounterState {
    ring: Ring,
    wm: WindowManager,
    #[lens(skip)]
    handle: WindowHandle<Counter>,
    #[lens(skip)]
    pointer: Point,
}

fn main() {
    let ring = Ring::default();
    let mut wm = WindowManager::new(ring.proxy());
    wm.upload_atlas(&atlas::COUNTER);

    let handle = wm.spawn_window(
        WindowSettings {
            width: 480,
            height: 240,
            clear_color: BG,
            kind: WindowKind::Xdg {
                title: "counter".into(),
            },
            touch_config: None,
            gesture_config: None,
        },
        Counter::new(),
    );

    let state = CounterState {
        ring,
        wm,
        handle,
        pointer: Point::new(-1.0, -1.0),
    };

    let mut app = app::App::new(state)
        .mount(io_ring::module())
        .mount(window_manager::module())
        .mount(app::Module::new().on(on_pointer));

    app.dispatch(&app::Start);
    loop {
        app.dispatch(&app::PrePoll);
        app.dispatch(&app::Poll);
    }
}

fn on_pointer(s: &mut CounterState, ev: &WlPointerEvent) {
    let handle = s.handle;
    match ev {
        WlPointerEvent::Enter {
            surface_x,
            surface_y,
            ..
        }
        | WlPointerEvent::Motion {
            surface_x,
            surface_y,
            ..
        } => {
            s.pointer = Point::new(*surface_x, *surface_y);
            handle.set(Hover(Some(s.pointer)), &mut s.wm);
        }
        WlPointerEvent::Leave { .. } => {
            handle.set(Hover(None), &mut s.wm);
        }
        WlPointerEvent::Button {
            state: WlPointerButtonState::Pressed,
            button,
            ..
        } if *button == BTN_LEFT => {
            handle.set(Click(s.pointer), &mut s.wm);
        }
        _ => {}
    }
}
