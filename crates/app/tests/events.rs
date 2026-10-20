//! Signals and systems, events and handlers, the queues, and the flush
//! order.

use std::cell::RefCell;

use app::prelude::*;

// ── fixtures ────────────────────────────────────────────────────────────

// Systems cannot capture, so every test logs through a thread-local.
// Each test runs on its own thread, so logs never mix.
thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(s: impl Into<String>) {
    LOG.with(|l| l.borrow_mut().push(s.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// A widget with nothing in it.
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

/// Spawns two `Leaf` children, so one spawn makes three nodes.
struct Branch;
impl Build for Branch {
    type Widget = Branch;
}
impl Widget for Branch {
    type Builder = Branch;
    fn build(b: Branch, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        s.spawn(me, Leaf);
        s.spawn(me, Leaf);
        b
    }
}

struct Ping;
impl Signal for Ping {}

struct Pong;
impl Signal for Pong {}

struct Inc;
impl Event for Inc {}

struct Add(u32);
impl Event for Add {}

struct Step;
impl Event for Step {}

/// Counts `Inc` and `Add` events on itself.
struct Counter(u32);
struct CounterBuilder;
impl Build for CounterBuilder {
    type Widget = Counter;
}
impl Widget for Counter {
    type Builder = CounterBuilder;
    fn build(_b: CounterBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        s.on::<Inc>(me, |ctx, _| ctx.me().0 += 1);
        s.on::<Add>(me, |ctx, e| ctx.me().0 += e.0);
        Counter(0)
    }
}

/// Logs `tag` whenever `Inc` lands on `target`, on its own behalf: the
/// handler's owner is the watcher, its target is someone else.
struct Watcher;
struct WatcherBuilder {
    target: NodeId,
    tag: &'static str,
}
impl Build for WatcherBuilder {
    type Widget = Watcher;
}
impl Widget for Watcher {
    type Builder = WatcherBuilder;
    fn build(b: WatcherBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let tag = b.tag;
        s.on::<Inc>(b.target, move |ctx, _| {
            log(format!(
                "{tag} target_is_me={}",
                ctx.target() == ctx.handle().id()
            ))
        });
        Watcher
    }
}

fn count(app: &App, h: Handle<Counter>) -> u32 {
    app.widget::<Counter>(h).unwrap().0
}

// ── systems ─────────────────────────────────────────────────────────────

#[test]
fn systems_run_in_registration_order_for_their_signal_only() {
    fn a(_: &mut App, _: &Ping) {
        log("a");
    }
    fn b(_: &mut App, _: &Ping) {
        log("b");
    }
    fn other(_: &mut App, _: &Pong) {
        log("pong");
    }

    let mut app = App::new();
    app.system(b).system(a).system(other);
    app.signal(Ping);
    assert!(take_log().is_empty(), "nothing runs before flush");
    app.flush();
    assert_eq!(take_log(), ["b", "a"]);
}

#[test]
fn the_same_system_twice_runs_twice() {
    fn a(_: &mut App, _: &Ping) {
        log("a");
    }
    let mut app = App::new();
    app.system(a).system(a);
    app.signal(Ping);
    app.flush();
    assert_eq!(take_log(), ["a", "a"]);
}

#[test]
fn a_signal_with_no_systems_is_dropped_when_sent() {
    fn a(_: &mut App, _: &Ping) {
        log("a");
    }
    let mut app = App::new();
    app.signal(Ping);
    // Registered after the send, before the flush: too late.
    app.system(a);
    app.flush();
    assert!(take_log().is_empty());
}

#[test]
fn a_system_registered_mid_pass_runs_in_that_pass() {
    fn first(app: &mut App, _: &Ping) {
        log("first");
        app.system(second);
    }
    fn second(_: &mut App, _: &Ping) {
        log("second");
    }
    let mut app = App::new();
    app.system(first);
    app.signal(Ping);
    app.flush();
    assert_eq!(take_log(), ["first", "second"]);
}

#[test]
fn a_system_can_spawn_remove_and_signal_and_it_all_runs_in_one_flush() {
    fn on_ping(app: &mut App, _: &Ping) {
        let root = app.root();
        app.spawn(root, Leaf);
        app.signal(Pong);
    }
    fn on_pong(app: &mut App, _: &Pong) {
        let ids: Vec<NodeId> = app.widgets::<Leaf>().map(|(id, _)| id).collect();
        log(format!("pong sees {}", ids.len()));
        for id in ids {
            app.remove(id);
        }
    }
    let mut app = App::new();
    app.system(on_ping).system(on_pong);
    app.signal(Ping);
    app.flush();
    assert_eq!(take_log(), ["pong sees 1"]);
    assert_eq!(app.widgets::<Leaf>().count(), 0);
}

#[test]
fn spawned_arrives_after_the_widget_is_stored_children_before_parent() {
    fn on_spawned(app: &mut App, s: &Spawned) {
        let kind = if app.widget::<Branch>(s.id).is_some() {
            "branch"
        } else if app.widget::<Leaf>(s.id).is_some() {
            "leaf"
        } else {
            "unfinished"
        };
        log(format!("{kind} under root={}", s.parent == app.root()));
    }
    let mut app = App::new();
    app.system(on_spawned);
    app.spawn(app.root(), Branch);
    app.flush();
    assert_eq!(
        take_log(),
        [
            "leaf under root=false",
            "leaf under root=false",
            "branch under root=true"
        ]
    );
}

#[test]
fn removed_arrives_once_per_remove_with_stale_ids() {
    fn on_removed(app: &mut App, r: &Removed) {
        log(format!(
            "removed live={} parent_is_root={}",
            app.is_live(r.id),
            r.parent == app.root()
        ));
    }
    let mut app = App::new();
    app.system(on_removed);
    let branch = app.spawn(app.root(), Branch);
    app.remove(branch);
    app.remove(branch); // stale: no signal
    app.flush();
    assert_eq!(take_log(), ["removed live=false parent_is_root=true"]);
}

#[test]
fn flush_on_empty_queues_is_a_noop() {
    let mut app = App::new();
    app.flush();
    app.flush();
}

// ── events and handlers ─────────────────────────────────────────────────

#[test]
fn emit_reaches_only_the_named_nodes_in_order() {
    let mut app = App::new();
    let a = app.spawn(app.root(), CounterBuilder);
    let b = app.spawn(app.root(), CounterBuilder);
    let c = app.spawn(a, CounterBuilder);
    app.spawn(
        app.root(),
        WatcherBuilder {
            target: c.id(),
            tag: "c",
        },
    );
    app.spawn(
        app.root(),
        WatcherBuilder {
            target: a.id(),
            tag: "a",
        },
    );

    app.emit(Inc, [c.id(), a.id()]);
    assert_eq!(count(&app, a), 0, "queued, not run");
    app.flush();
    assert_eq!((count(&app, a), count(&app, b), count(&app, c)), (1, 0, 1));
    assert_eq!(take_log(), ["c target_is_me=false", "a target_is_me=false"]);
}

#[test]
fn emit_all_is_queued_and_visits_the_root_then_descendants_in_tree_order() {
    let mut app = App::new();
    let root = app.root();
    let a = app.spawn(root, CounterBuilder);
    let b = app.spawn(root, CounterBuilder);
    let c = app.spawn(a, CounterBuilder);
    let d = app.spawn(a, CounterBuilder);
    // Slot order is a, b, c, d; tree order is a, c, d, b.
    for (target, tag) in [
        (root, "root"),
        (a.id(), "a"),
        (b.id(), "b"),
        (c.id(), "c"),
        (d.id(), "d"),
    ] {
        app.spawn(root, WatcherBuilder { target, tag });
    }
    app.spawn(root, Leaf); // Nodes without handlers are harmless.

    app.emit_all(Inc);
    assert!(take_log().is_empty(), "nothing runs before flush");
    assert_eq!(count(&app, a), 0);
    app.flush();

    assert_eq!(
        take_log(),
        [
            "root target_is_me=false",
            "a target_is_me=false",
            "c target_is_me=false",
            "d target_is_me=false",
            "b target_is_me=false",
        ]
    );
    for handle in [a, b, c, d] {
        assert_eq!(count(&app, handle), 1, "each node receives the event once");
    }
}

#[test]
fn emit_all_snapshots_targets_and_reports_them_even_if_removed_before_flush() {
    #[derive(Default)]
    struct Emissions(Vec<(u32, Vec<NodeId>)>);
    impl Resource for Emissions {}

    fn on_emitted(app: &mut App, e: &Emitted<Add>) {
        for &id in e.targets.iter() {
            if let Some(counter) = app.widget::<Counter>(id) {
                assert_eq!(counter.0, e.event.0, "handlers ran before the signal");
            }
        }
        app.resource_mut::<Emissions>()
            .0
            .push((e.event.0, e.targets.to_vec()));
    }

    let mut app = App::new();
    app.init_resource::<Emissions>().system(on_emitted);
    let root = app.root();
    let kept = app.spawn(root, CounterBuilder);
    let gone = app.spawn(root, CounterBuilder);
    let unhandled = app.spawn(root, Leaf);

    app.emit_all(Add(7));
    app.remove(gone);
    let replacement = app.spawn(root, CounterBuilder);
    let late = app.spawn(kept, CounterBuilder);
    assert!(app.resource::<Emissions>().0.is_empty());
    app.flush();

    assert_eq!(count(&app, kept), 7);
    assert_eq!(count(&app, replacement), 0, "a reused slot is not a target");
    assert_eq!(
        count(&app, late),
        0,
        "nodes spawned after emit_all are not targets"
    );
    assert_eq!(
        app.resource::<Emissions>().0,
        [(7, vec![root, kept.id(), gone.id(), unhandled.id()]),],
        "one signal retains the original event and full target snapshot"
    );
}

#[test]
fn emit_all_on_a_root_only_tree_still_emits_once() {
    fn on_emitted(app: &mut App, e: &Emitted<Add>) {
        assert_eq!(&*e.targets, &[app.root()]);
        assert_eq!(e.event.0, 3);
        log("emitted");
    }

    let mut app = App::new();
    app.system(on_emitted);
    app.emit_all(Add(3));
    assert!(take_log().is_empty());
    app.flush();
    assert_eq!(take_log(), ["emitted"]);
}

#[test]
fn emit_all_skips_nodes_removed_by_an_earlier_handler() {
    struct Reaper;
    struct ReaperBuilder {
        root: NodeId,
        victim: NodeId,
    }
    impl Build for ReaperBuilder {
        type Widget = Reaper;
    }
    impl Widget for Reaper {
        type Builder = ReaperBuilder;
        fn build(b: ReaperBuilder, _: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Inc>(b.root, move |ctx, _| {
                ctx.remove(b.victim);
                ctx.spawn(b.root, CounterBuilder);
                log("reaped");
            });
            Reaper
        }
    }

    let mut app = App::new();
    let gone = app.spawn(app.root(), CounterBuilder);
    app.spawn(gone, CounterBuilder);
    let kept = app.spawn(app.root(), CounterBuilder);
    app.spawn(
        app.root(),
        ReaperBuilder {
            root: app.root(),
            victim: gone.id(),
        },
    );
    app.emit_all(Inc);
    app.flush();

    assert!(!app.is_live(gone));
    assert_eq!(count(&app, kept), 1);
    assert_eq!(
        app.widgets::<Counter>().map(|(_, c)| c.0).sum::<u32>(),
        1,
        "the replacement spawned during dispatch receives no event"
    );
    assert_eq!(take_log(), ["reaped"]);
}

#[test]
fn emit_to_the_same_node_twice_runs_its_handlers_twice() {
    let mut app = App::new();
    let a = app.spawn(app.root(), CounterBuilder);
    app.emit(Inc, [a.id(), a.id()]);
    app.emit(Add(5), a);
    app.flush();
    assert_eq!(count(&app, a), 7);
}

#[test]
fn unhandled_root_stale_and_removed_since_emit_targets_are_skipped() {
    let mut app = App::new();
    let a = app.spawn(app.root(), CounterBuilder);
    let leaf = app.spawn(app.root(), Leaf);
    let gone = app.spawn(app.root(), CounterBuilder);
    app.remove(gone);
    // Reuses gone's freed slot; kept alive so its handler count can tell
    // apart a correctly skipped reused slot from a removed node.
    let late = app.spawn(app.root(), CounterBuilder);
    let soon = app.spawn(app.root(), CounterBuilder);

    app.emit(Inc, [leaf.id(), app.root(), gone.id(), soon.id(), a.id()]);
    app.remove(soon); // live when queued, stale by the time flush runs
    app.flush();
    assert_eq!(count(&app, a), 1);
    assert_eq!(
        count(&app, late),
        0,
        "gone's stale id names late's slot but must not run late's handler"
    );
}

#[test]
fn handlers_for_one_node_and_event_run_in_registration_order() {
    struct Order;
    struct OrderBuilder;
    impl Build for OrderBuilder {
        type Widget = Order;
    }
    impl Widget for Order {
        type Builder = OrderBuilder;
        fn build(_b: OrderBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Step>(me, |_, _| log("1"))
                .on::<Step>(me, |_, _| log("2"))
                .on::<Step>(me, |_, _| log("3"));
            Order
        }
    }
    let mut app = App::new();
    let o = app.spawn(app.root(), OrderBuilder);
    app.emit(Step, o);
    app.flush();
    assert_eq!(take_log(), ["1", "2", "3"]);
}

#[test]
#[should_panic(expected = "stale target")]
fn on_with_a_stale_target_panics() {
    let mut app = App::new();
    let gone = app.spawn(app.root(), CounterBuilder);
    app.remove(gone);
    app.spawn(
        app.root(),
        WatcherBuilder {
            target: gone.id(),
            tag: "never",
        },
    );
}

#[test]
fn a_parent_can_handle_events_on_its_child_with_itself_as_me() {
    struct Row(u32);
    struct RowBuilder;
    impl Build for RowBuilder {
        type Widget = Row;
    }
    impl Widget for Row {
        type Builder = RowBuilder;
        fn build(_b: RowBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            let child = s.spawn(me, CounterBuilder);
            s.on::<Inc>(child, |ctx, _| {
                ctx.me().0 += 10;
                let target = ctx.target();
                let child_count = ctx.widget::<Counter>(target).unwrap().0;
                log(format!("row sees child at {child_count}"));
            });
            Row(0)
        }
    }
    let mut app = App::new();
    let row = app.spawn(app.root(), RowBuilder);
    let child = app.children(row).unwrap()[0];
    app.emit(Inc, child);
    app.flush();
    assert_eq!(app.widget::<Row>(row).unwrap().0, 10);
    // The child's own handler ran first: it was registered first.
    assert_eq!(take_log(), ["row sees child at 1"]);
}

#[test]
#[should_panic(expected = "a handler's owner is live and holds its widget")]
fn me_panics_if_the_handler_removed_its_own_owner() {
    struct SelfDestruct;
    struct SelfDestructBuilder;
    impl Build for SelfDestructBuilder {
        type Widget = SelfDestruct;
    }
    impl Widget for SelfDestruct {
        type Builder = SelfDestructBuilder;
        fn build(_b: SelfDestructBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Inc>(me, |ctx, _| {
                let handle = ctx.handle();
                ctx.remove(handle);
                ctx.me();
            });
            SelfDestruct
        }
    }
    let mut app = App::new();
    let d = app.spawn(app.root(), SelfDestructBuilder);
    app.emit(Inc, d);
    app.flush();
}

#[test]
fn a_handler_whose_owner_was_removed_does_not_run() {
    let mut app = App::new();
    let c = app.spawn(app.root(), CounterBuilder);
    let w = app.spawn(
        app.root(),
        WatcherBuilder {
            target: c.id(),
            tag: "w",
        },
    );
    app.remove(w);
    app.emit(Inc, c);
    app.flush();
    assert_eq!(count(&app, c), 1, "the counter's own handler still runs");
    assert!(take_log().is_empty(), "the watcher's does not");
}

#[test]
fn at_reaches_another_node_and_is_none_for_a_stale_handle() {
    struct Reader;
    struct Look(Handle<Counter>, Handle<Counter>);
    impl Event for Look {}
    struct ReaderBuilder;
    impl Build for ReaderBuilder {
        type Widget = Reader;
    }
    impl Widget for Reader {
        type Builder = ReaderBuilder;
        fn build(_b: ReaderBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Look>(me, |ctx, e| {
                let seen = ctx.at(e.0).map(|mut other| {
                    other.me().0 = 99;
                    other.handle() == e.0 && other.target() == e.0.id()
                });
                let stale = ctx.at(e.1).is_none();
                log(format!("seen={seen:?} stale={stale}"));
            });
            Reader
        }
    }
    let mut app = App::new();
    let live = app.spawn(app.root(), CounterBuilder);
    let gone = app.spawn(app.root(), CounterBuilder);
    app.remove(gone);
    let r = app.spawn(app.root(), ReaderBuilder);
    app.emit(Look(live, gone), r);
    app.flush();
    assert_eq!(take_log(), ["seen=Some(true) stale=true"]);
    assert_eq!(count(&app, live), 99);
}

#[test]
fn a_handler_can_read_and_write_its_owners_components() {
    #[derive(Default, Debug, PartialEq)]
    struct Hits(u32);
    impl Component for Hits {}

    struct Tally;
    struct TallyBuilder;
    impl Build for TallyBuilder {
        type Widget = Tally;
    }
    impl Widget for Tally {
        type Builder = TallyBuilder;
        fn build(_b: TallyBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Step>(me, |ctx, _| {
                let before = ctx.component::<Hits>().unwrap().0;
                ctx.component_mut::<Hits>().unwrap().0 = before + 1;
            });
            Tally
        }
    }
    let mut app = App::new();
    app.register_component::<Hits>();
    let t = app.spawn(app.root(), TallyBuilder);
    app.emit(Step, [t.id(), t.id()]);
    app.flush();
    assert_eq!(app.component::<Hits>(t), Some(&Hits(2)));
    assert_eq!(app.take_changed::<Hits>().collect::<Vec<_>>(), [t.id()]);
}

#[test]
fn a_handler_adding_a_handler_to_its_target_keeps_both_in_order() {
    struct Adder;
    struct AdderBuilder;
    impl Build for AdderBuilder {
        type Widget = Adder;
    }
    impl Widget for Adder {
        type Builder = AdderBuilder;
        fn build(_b: AdderBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Inc>(me, |ctx, _| {
                log("adder");
                let target = ctx.target();
                let root = ctx.tree().root();
                ctx.spawn(
                    root,
                    WatcherBuilder {
                        target,
                        tag: "added",
                    },
                );
            });
            Adder
        }
    }
    let mut app = App::new();
    let a = app.spawn(app.root(), AdderBuilder);
    app.emit(Inc, a);
    app.flush();
    assert_eq!(
        take_log(),
        ["adder"],
        "a handler added mid-call waits for the next emit"
    );
    app.emit(Inc, a);
    app.flush();
    assert_eq!(take_log(), ["adder", "added target_is_me=false"]);
}

#[test]
fn a_reused_slot_inherits_no_handlers_from_the_node_a_handler_removed() {
    // The parent owns the handler and stays live, so if the child's list
    // were wrongly put back onto the reused slot, the handler would run
    // again for the new node.
    struct Reaper;
    struct ReaperBuilder;
    impl Build for ReaperBuilder {
        type Widget = Reaper;
    }
    impl Widget for Reaper {
        type Builder = ReaperBuilder;
        fn build(_b: ReaperBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            let child = s.spawn(me, Leaf);
            s.on::<Inc>(child, |ctx, _| {
                log("reap");
                let target = ctx.target();
                let root = ctx.tree().root();
                ctx.remove(target);
                ctx.spawn(root, Leaf);
            });
            Reaper
        }
    }
    let mut app = App::new();
    let reaper = app.spawn(app.root(), ReaperBuilder);
    let child = app.children(reaper).unwrap()[0];
    app.emit(Inc, child);
    app.flush();
    assert_eq!(take_log(), ["reap"]);
    assert!(!app.is_live(child));
    let reused = *app.children(app.root()).unwrap().last().unwrap();
    assert_ne!(reused, child);
    app.emit(Inc, reused);
    app.flush();
    assert!(take_log().is_empty(), "the new node has no handlers");
}

#[test]
fn a_panicking_handler_leaves_the_widget_and_the_other_handlers_in_place() {
    struct Fuse(u32);
    struct Blow;
    impl Event for Blow {}
    struct FuseBuilder;
    impl Build for FuseBuilder {
        type Widget = Fuse;
    }
    impl Widget for Fuse {
        type Builder = FuseBuilder;
        fn build(_b: FuseBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            s.on::<Blow>(me, |ctx, _| {
                ctx.me().0 += 1;
                if ctx.me().0 == 1 {
                    panic!("blown");
                }
            })
            .on::<Blow>(me, |_, _| log("second"));
            Fuse(0)
        }
    }

    let mut app = App::new();
    let f = app.spawn(app.root(), FuseBuilder);
    app.emit(Blow, f);

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.flush()));
    std::panic::set_hook(hook);
    assert!(result.is_err(), "the panic propagates out of flush");

    assert_eq!(
        app.widget::<Fuse>(f).unwrap().0,
        1,
        "the write before the panic stuck"
    );
    app.emit(Blow, f);
    app.flush();
    assert_eq!(app.widget::<Fuse>(f).unwrap().0, 2);
    assert_eq!(take_log(), ["second"], "both handlers survived the unwind");
}

// ── flush order ─────────────────────────────────────────────────────────

#[test]
fn emitted_reaches_systems_after_the_handlers_with_the_same_targets() {
    fn on_emitted(app: &mut App, e: &Emitted<Add>) {
        let total: u32 = e
            .targets
            .iter()
            .map(|&id| app.widget::<Counter>(id).unwrap().0)
            .sum();
        log(format!(
            "emitted {} to {} nodes, total now {total}",
            e.event.0,
            e.targets.len()
        ));
    }
    let mut app = App::new();
    app.system(on_emitted);
    let a = app.spawn(app.root(), CounterBuilder);
    let b = app.spawn(app.root(), CounterBuilder);
    app.spawn(
        app.root(),
        WatcherBuilder {
            target: a.id(),
            tag: "w",
        },
    );
    app.emit(Add(2), [a.id(), b.id()]);
    app.emit(Inc, a);
    app.flush();
    // Both events ran (events drain before any signal), then the signal.
    assert_eq!(
        take_log(),
        ["w target_is_me=false", "emitted 2 to 2 nodes, total now 5"]
    );
}

#[test]
fn a_handler_can_emit_and_signal_and_its_event_runs_before_its_signal() {
    struct Relay;
    struct Kick;
    impl Event for Kick {}
    struct RelayBuilder(Handle<Counter>);
    impl Build for RelayBuilder {
        type Widget = Relay;
    }
    impl Widget for Relay {
        type Builder = RelayBuilder;
        fn build(b: RelayBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
            let counter = b.0;
            s.on::<Kick>(me, move |ctx, _| {
                ctx.signal(Ping);
                ctx.emit(Inc, counter);
                log("kick");
            });
            Relay
        }
    }
    fn on_ping(app: &mut App, _: &Ping) {
        let total: u32 = app.widgets::<Counter>().map(|(_, c)| c.0).sum();
        log(format!("ping {total}"));
    }

    let mut app = App::new();
    app.system(on_ping);
    let c = app.spawn(app.root(), CounterBuilder);
    let r = app.spawn(app.root(), RelayBuilder(c));
    app.emit(Kick, r);
    app.flush();
    assert_eq!(take_log(), ["kick", "ping 1"]);
}

#[test]
fn flush_runs_pending_events_before_each_signal() {
    fn on_ping(app: &mut App, _: &Ping) {
        let ids: Vec<NodeId> = app.widgets::<Counter>().map(|(id, _)| id).collect();
        log(format!(
            "ping sees {}",
            app.widget::<Counter>(ids[0]).unwrap().0
        ));
        app.emit(Inc, ids);
        app.signal(Pong);
    }
    fn on_pong(app: &mut App, _: &Pong) {
        let v = app.widgets::<Counter>().map(|(_, c)| c.0).next().unwrap();
        log(format!("pong sees {v}"));
    }
    let mut app = App::new();
    app.system(on_ping).system(on_pong);
    let c = app.spawn(app.root(), CounterBuilder);

    // Queued as Ping then Inc; events go first, so Ping sees the Inc.
    app.signal(Ping);
    app.emit(Inc, c);
    app.flush();
    assert_eq!(take_log(), ["ping sees 1", "pong sees 2"]);
}

// ── tick and runner ─────────────────────────────────────────────────────

#[test]
fn tick_sends_tick_then_post_tick_and_flushes() {
    fn on_tick(app: &mut App, _: &Tick) {
        log("tick");
        app.signal(Ping);
    }
    fn on_ping(_: &mut App, _: &Ping) {
        log("ping");
    }
    fn on_post(_: &mut App, _: &PostTick) {
        log("post");
    }
    let mut app = App::new();
    app.system(on_post).system(on_tick).system(on_ping);
    app.tick();
    // Ping was queued behind PostTick, so it runs after it.
    assert_eq!(take_log(), ["tick", "post", "ping"]);
}

#[test]
fn run_hands_the_app_to_the_runner_and_returns_when_it_does() {
    fn once(mut app: App) {
        app.tick();
        log(format!(
            "ran with {} counters",
            app.widgets::<Counter>().count()
        ));
    }
    fn on_tick(_: &mut App, _: &Tick) {
        log("tick");
    }
    let mut app = App::new();
    app.system(on_tick).set_runner(once);
    app.spawn(app.root(), CounterBuilder);
    app.run();
    assert_eq!(take_log(), ["tick", "ran with 1 counters"]);
}

#[test]
#[should_panic(expected = "runner already set")]
fn setting_the_runner_twice_panics() {
    let mut app = App::new();
    app.set_runner(|_| {}).set_runner(|_| {});
}
