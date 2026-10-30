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
    Interval(Duration),
}

/// What a `Poke` asks the probe's handler to do through `Context`.
#[derive(Clone, Copy)]
enum Cmd {
    Start,
    StartFor(Duration),
    Cancel,
    CancelThenStart,
    /// Call all four methods and record what they returned.
    All,
}

/// Drive `ContextTimers` from a handler: `id` defaults to the probe's
/// first timer.
struct Poke {
    cmd: Cmd,
    id: Option<TimerId>,
}
impl Event for Poke {}

/// What the `ContextTimers` calls of a `Cmd::All` returned.
#[derive(Default)]
struct Results(Vec<bool>);
impl Resource for Results {}

struct Probe {
    ids: Vec<TimerId>,
}
struct ProbeBuilder(Setup);
impl Build for ProbeBuilder {
    type Widget = Probe;
}
impl Widget for Probe {
    type Builder = ProbeBuilder;
    fn build(b: ProbeBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let ids = match b.0 {
            Setup::Timeout(d) => vec![s.set_timeout(d, log)],
            Setup::AddTimeout(d) => vec![s.add_timeout(d, log)],
            Setup::Two(a, b) => vec![s.set_timeout(a, log), s.set_timeout(b, log)],
            Setup::Interval(d) => vec![s.set_interval(d, log)],
        };
        s.resource_mut::<Ids>().0.extend(ids.iter().copied());
        s.on::<Poke>(me, |ctx, p| {
            let id = p.id.unwrap_or(ctx.me().ids[0]);
            match p.cmd {
                Cmd::Start => {
                    ctx.start_timer(id);
                }
                Cmd::StartFor(d) => {
                    ctx.start_timer_for(id, d);
                }
                Cmd::Cancel => {
                    ctx.cancel_timer(id);
                }
                Cmd::CancelThenStart => {
                    ctx.cancel_timer(id);
                    ctx.start_timer(id);
                }
                Cmd::All => {
                    let r = vec![
                        ctx.start_timer(id),
                        ctx.start_timer_for(id, Duration::from_millis(5)),
                        ctx.cancel_timer(id),
                        ctx.timer_running(id),
                    ];
                    ctx.resource_mut::<Results>().0 = r;
                }
            }
        });
        Probe { ids }
    }
}

fn probe(app: &mut App, setup: Setup) -> Handle<Probe> {
    let root = app.root();
    app.spawn(root, ProbeBuilder(setup))
}

fn poke(app: &mut App, at: Handle<Probe>, cmd: Cmd) {
    app.emit(Poke { cmd, id: None }, at);
    app.flush();
}

/// Submit pending ops without blocking: the `BeforeWait` half of a turn.
fn submit(app: &mut App) {
    app.signal(BeforeWait);
    app.flush();
}

fn gaps(fires: &[(TimerId, Instant)]) -> Vec<Duration> {
    fires.windows(2).map(|w| w[1].1 - w[0].1).collect()
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

#[test]
fn interval_repeats() {
    let mut app = app();
    probe(&mut app, Setup::Interval(Duration::from_millis(20)));
    run_for(&mut app, Duration::from_millis(110));
    let fires = fires(&app);
    assert!(
        (4..=6).contains(&fires.len()),
        "fired {} times",
        fires.len()
    );
    for g in gaps(&fires) {
        assert!(
            g >= Duration::from_millis(19),
            "gap {g:?} shorter than the period"
        );
    }
}

#[test]
fn restart_debounces() {
    let mut app = app();
    let p = probe(&mut app, Setup::AddTimeout(Duration::from_millis(40)));
    let first = Instant::now();
    for _ in 0..3 {
        poke(&mut app, p, Cmd::Start);
        submit(&mut app);
        std::thread::sleep(Duration::from_millis(15));
    }
    run_for(&mut app, Duration::from_millis(150));
    let fires = fires(&app);
    assert_eq!(fires.len(), 1, "only the last start fires");
    assert!(
        fires[0].1 - first >= Duration::from_millis(70),
        "due 40ms after the third start"
    );
}

#[test]
fn cancel_prevents_fire() {
    let mut app = app();
    let p = probe(&mut app, Setup::Timeout(Duration::from_millis(30)));
    submit(&mut app);
    poke(&mut app, p, Cmd::Cancel);
    run_for(&mut app, Duration::from_millis(100));
    assert!(fires(&app).is_empty());
    assert!(!app.resource::<Timers>().is_running(ids(&app)[0]));
}

#[test]
fn cancel_then_start_fires_once() {
    let mut app = app();
    let p = probe(&mut app, Setup::Timeout(Duration::from_millis(30)));
    submit(&mut app);
    poke(&mut app, p, Cmd::CancelThenStart);
    run_for(&mut app, Duration::from_millis(150));
    assert_eq!(fires(&app).len(), 1);
}

#[test]
fn start_timer_for_changes_period() {
    let mut app = app();
    let p = probe(&mut app, Setup::Interval(Duration::from_millis(50)));
    while fires(&app).is_empty() {
        run_for(&mut app, Duration::from_millis(10));
    }
    poke(&mut app, p, Cmd::StartFor(Duration::from_millis(15)));
    let before = fires(&app).len();
    run_for(&mut app, Duration::from_millis(100));
    let after = &fires(&app)[before..];
    assert!(
        after.len() >= 4,
        "fired {} times at the new period",
        after.len()
    );
    for g in gaps(after) {
        assert!(
            g >= Duration::from_millis(14) && g < Duration::from_millis(40),
            "gap {g:?}"
        );
    }
}

#[test]
fn shorter_retime_fires_early() {
    let mut app = app();
    let p = probe(&mut app, Setup::Timeout(Duration::from_millis(500)));
    submit(&mut app);
    let poked = Instant::now();
    poke(&mut app, p, Cmd::StartFor(Duration::from_millis(20)));
    app.tick();
    Ring::turn(&mut app);
    let fires = fires(&app);
    assert_eq!(fires.len(), 1, "the shorter deadline fired");
    assert!(fires[0].1 - poked < Duration::from_millis(150));
}

#[test]
fn restarts_do_not_grow_in_flight() {
    let mut app = app();
    let p = probe(&mut app, Setup::AddTimeout(Duration::from_secs(30)));
    for _ in 0..200 {
        poke(&mut app, p, Cmd::Start);
        app.tick();
        submit(&mut app);
        let n = app.resource::<Timers>().in_flight();
        assert!(n <= 2, "{n} ops in flight");
    }
}

#[test]
fn zero_timeout_fires_once() {
    let mut app = app();
    probe(&mut app, Setup::Timeout(Duration::ZERO));
    run_for(&mut app, Duration::from_millis(50));
    assert_eq!(fires(&app).len(), 1);
}

#[test]
#[should_panic(expected = "zero-length interval")]
fn zero_interval_panics() {
    let mut app = app();
    probe(&mut app, Setup::Interval(Duration::ZERO));
}

#[test]
fn unknown_id_is_noop() {
    let mut other = app();
    probe(
        &mut other,
        Setup::Two(Duration::from_secs(1), Duration::from_secs(1)),
    );
    let foreign = ids(&other)[1];

    let mut app = app();
    app.init_resource::<Results>();
    let p = probe(&mut app, Setup::AddTimeout(Duration::from_secs(1)));
    assert_ne!(ids(&app), vec![foreign], "the id is unknown here");
    app.emit(
        Poke {
            cmd: Cmd::All,
            id: Some(foreign),
        },
        p,
    );
    app.flush();
    assert_eq!(app.resource::<Results>().0, vec![false; 4]);
}
