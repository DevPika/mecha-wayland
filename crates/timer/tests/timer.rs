use std::time::{Duration, Instant};

use app::prelude::*;
use ring::prelude::*;
use timer::prelude::*;

/// Every fire, in order: which timer and when.
#[derive(Default)]
struct Log(Vec<(TimerId, Instant)>);
impl Resource for Log {}

/// The ids `Probe::build` registered, in registration order.
#[derive(Default)]
struct Ids(Vec<TimerId>);
impl Resource for Ids {}

fn app() -> App {
    let mut app = App::new();
    app.add_module(RingModule::default())
        .add_module(TimerModule)
        .init_resource::<Log>()
        .init_resource::<Ids>();
    app
}

/// Tick and turn until `d` has passed or no timer is pending. Never turns
/// with nothing in flight: with only the ring module installed, that
/// turn would block forever.
fn run_for(app: &mut App, d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        app.tick();
        if app.resource::<Timers>().in_flight() == 0 {
            break;
        }
        Ring::turn(app);
    }
}

fn fires(app: &App) -> Vec<(TimerId, Instant)> {
    app.resource::<Log>().0.clone()
}

fn ids(app: &App) -> Vec<TimerId> {
    app.resource::<Ids>().0.clone()
}

fn log(ctx: &mut Context<'_, Probe>, e: &Timeout) {
    ctx.resource_mut::<Log>().0.push((e.id, Instant::now()));
}

/// Which timers a `Probe` registers in its build.
enum Setup {
    Timeout(Duration),
    AddTimeout(Duration),
    Two(Duration, Duration),
}

struct Probe;
struct ProbeBuilder(Setup);
impl Build for ProbeBuilder {
    type Widget = Probe;
}
impl Widget for Probe {
    type Builder = ProbeBuilder;
    fn build(b: ProbeBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let ids = match b.0 {
            Setup::Timeout(d) => vec![s.set_timeout(d, log)],
            Setup::AddTimeout(d) => vec![s.add_timeout(d, log)],
            Setup::Two(a, b) => vec![s.set_timeout(a, log), s.set_timeout(b, log)],
        };
        s.resource_mut::<Ids>().0.extend(ids);
        Probe
    }
}

fn probe(app: &mut App, setup: Setup) -> Handle<Probe> {
    let root = app.root();
    app.spawn(root, ProbeBuilder(setup))
}

#[test]
fn timeout_fires_once() {
    let mut app = app();
    let start = Instant::now();
    probe(&mut app, Setup::Timeout(Duration::from_millis(20)));
    run_for(&mut app, Duration::from_millis(150));
    let fires = fires(&app);
    assert_eq!(fires.len(), 1, "one-shot fires exactly once");
    assert_eq!(fires[0].0, ids(&app)[0]);
    assert!(
        fires[0].1 - start >= Duration::from_millis(20),
        "not before its deadline"
    );
}

#[test]
fn add_timeout_waits() {
    let mut app = app();
    probe(&mut app, Setup::AddTimeout(Duration::from_millis(20)));
    run_for(&mut app, Duration::from_millis(80));
    assert!(
        fires(&app).is_empty(),
        "a registered-stopped timer never fires"
    );
    let id = ids(&app)[0];
    assert!(!app.resource::<Timers>().is_running(id));
    assert_eq!(app.resource::<Timers>().in_flight(), 0);
}

#[test]
fn idle_app_is_woken() {
    let mut app = app();
    probe(&mut app, Setup::Timeout(Duration::from_millis(30)));
    app.tick();
    Ring::turn(&mut app);
    assert_eq!(
        fires(&app).len(),
        1,
        "the turn's block was ended by our timeout"
    );
}

#[test]
fn handler_only_sees_its_timer() {
    let mut app = app();
    probe(
        &mut app,
        Setup::Two(Duration::from_millis(10), Duration::from_millis(30)),
    );
    run_for(&mut app, Duration::from_millis(150));
    let got: Vec<TimerId> = fires(&app).iter().map(|f| f.0).collect();
    assert_eq!(
        got,
        ids(&app),
        "each handler ran once, for its own timer, in deadline order"
    );
}

#[test]
#[should_panic(expected = "TimerModule installs after RingModule")]
fn module_needs_ring() {
    App::new().add_module(TimerModule);
}
