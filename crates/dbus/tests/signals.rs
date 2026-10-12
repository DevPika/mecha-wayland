use app::prelude::*;
use dbus::fake::FakeBus;
use dbus::*;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

#[derive(Debug, Clone)]
enum Ev {
    Reply(MethodReply),
    Signal(SignalReceived),
    Acquired(String),
    Failed(MatchId),
}

#[derive(Default)]
struct Log(Vec<Ev>);
impl Resource for Log {}

fn on_reply(app: &mut App, e: &MethodReply) {
    app.resource_mut::<Log>().0.push(Ev::Reply(e.clone()));
}
fn on_signal(app: &mut App, e: &SignalReceived) {
    app.resource_mut::<Log>().0.push(Ev::Signal(e.clone()));
}
fn on_acquired(app: &mut App, e: &NameAcquired) {
    app.resource_mut::<Log>()
        .0
        .push(Ev::Acquired(e.name.as_str().to_owned()));
}
fn on_failed(app: &mut App, e: &MatchFailed) {
    app.resource_mut::<Log>().0.push(Ev::Failed(e.id));
}

fn fake() -> FakeBus {
    let mut f = FakeBus::new();
    f.app
        .init_resource::<Log>()
        .system(on_reply)
        .system(on_signal)
        .system(on_acquired)
        .system(on_failed);
    f.received();
    f
}

fn take(f: &mut FakeBus) -> Vec<Ev> {
    std::mem::take(&mut f.app.resource_mut::<Log>().0)
}

fn signals(f: &mut FakeBus) -> Vec<SignalReceived> {
    take(f)
        .into_iter()
        .filter_map(|e| match e {
            Ev::Signal(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn add(f: &mut FakeBus, rule: MatchRule) -> MatchId {
    f.app.resource_mut::<DBus>().session().add_match(rule)
}

fn from(sender: &str, path: &str, iface: &str, member: &str) -> Message {
    Message::signal(
        ObjectPath::new(path).unwrap(),
        InterfaceName::new(iface).unwrap(),
        MemberName::new(member).unwrap(),
    )
    .with_sender(BusName::new(sender).unwrap())
}

fn named(f: &mut FakeBus, member: &str) -> Vec<Message> {
    f.received()
        .into_iter()
        .filter(|m| m.member().is_some_and(|x| x.as_str() == member))
        .collect()
}

#[test]
fn one_signal_lists_every_rule_it_matches() {
    let mut f = fake();
    let a = add(&mut f, MatchRule::signal().member("Changed"));
    let b = add(&mut f, MatchRule::signal().interface("a.b"));
    let _c = add(&mut f, MatchRule::signal().interface("x.y"));
    f.turn();
    f.send(from(":1.9", "/p", "a.b", "Changed"));
    f.send(from(":1.9", "/p", "q.r", "Other"));
    f.turn();
    let s = signals(&mut f);
    assert_eq!(s.len(), 1, "the unmatched signal raises nothing");
    assert_eq!(s[0].matches.as_slice(), [a, b]);
    assert_eq!(s[0].msg.member().unwrap().as_str(), "Changed");
}

#[test]
fn identical_rules_share_one_add_match() {
    let mut f = fake();
    let rule = MatchRule::signal().interface("a.b");
    let a = add(&mut f, rule.clone());
    let b = add(&mut f, rule);
    f.turn();
    assert_eq!(named(&mut f, "AddMatch").len(), 1);
    f.app.resource_mut::<DBus>().session().remove_match(a);
    f.turn();
    assert!(
        named(&mut f, "RemoveMatch").is_empty(),
        "one rule still uses it"
    );
    f.app.resource_mut::<DBus>().session().remove_match(b);
    f.turn();
    let removes = named(&mut f, "RemoveMatch");
    assert_eq!(removes.len(), 1);
    assert_eq!(
        removes[0].body::<(String,)>().unwrap().0,
        "type='signal',interface='a.b'"
    );
}

#[test]
fn a_well_known_sender_follows_its_owner() {
    let mut f = fake();
    f.owners.insert("org.bluez".into(), ":1.5".into());
    let id = add(&mut f, MatchRule::signal().sender("org.bluez").member("M"));
    f.turn();
    assert_eq!(
        named(&mut f, "GetNameOwner").len(),
        1,
        "the owner is looked up"
    );
    f.send(from(":1.5", "/", "a.b", "M"));
    f.send(from(":1.6", "/", "a.b", "M"));
    f.turn();
    let s = signals(&mut f);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].msg.sender().unwrap().as_str(), ":1.5");
    assert_eq!(s[0].matches.as_slice(), [id]);

    f.send(
        Message::signal(
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameOwnerChanged",
        )
        .with_sender("org.freedesktop.DBus")
        .with_body(&("org.bluez", ":1.5", ":1.7")),
    );
    f.send(from(":1.5", "/", "a.b", "M"));
    f.send(from(":1.7", "/", "a.b", "M"));
    f.turn();
    let s = signals(&mut f);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].msg.sender().unwrap().as_str(), ":1.7");
}

#[test]
fn the_owner_is_looked_up_once_per_name() {
    let mut f = fake();
    f.owners.insert("org.bluez".into(), ":1.5".into());
    add(&mut f, MatchRule::signal().sender("org.bluez").member("A"));
    add(&mut f, MatchRule::signal().sender("org.bluez").member("B"));
    f.turn();
    assert_eq!(named(&mut f, "GetNameOwner").len(), 1);
}

#[test]
fn a_rule_can_wait_for_a_path_that_does_not_exist_yet() {
    let mut f = fake();
    f.owners
        .insert("org.freedesktop.portal.Desktop".into(), ":1.20".into());
    let request = "/org/freedesktop/portal/desktop/request/1_1/t1";
    let id = add(
        &mut f,
        MatchRule::signal()
            .sender("org.freedesktop.portal.Desktop")
            .path(ObjectPath::new(request).unwrap())
            .interface("org.freedesktop.portal.Request")
            .member("Response"),
    );
    let call = f
        .app
        .resource_mut::<DBus>()
        .session()
        .object(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
        )
        .call(
            "org.freedesktop.portal.FileChooser",
            "OpenFile",
            &("", "Pick", PropMap::new()),
        );
    f.turn();
    let open = f.expect_call("OpenFile");
    f.reply(&open, &(ObjectPath::new(request).unwrap(),));
    f.send(
        from(
            ":1.20",
            request,
            "org.freedesktop.portal.Request",
            "Response",
        )
        .with_body(&(0u32, PropMap::new())),
    );
    f.turn();
    let log = take(&mut f);
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(matches!(&log[0], Ev::Reply(r) if r.call == call));
    assert!(matches!(&log[1], Ev::Signal(s) if s.matches.as_slice() == [id]));
}

#[test]
fn a_rejected_rule_raises_match_failed() {
    let mut f = fake();
    f.reject_add_match = true;
    let id = add(&mut f, MatchRule::signal().member("X"));
    f.turn();
    let log = take(&mut f);
    assert!(matches!(&log[..], [Ev::Failed(x)] if *x == id), "{log:?}");
}

#[test]
fn requesting_a_name_replies_and_acquires_it() {
    let mut f = fake();
    let call = f
        .app
        .resource_mut::<DBus>()
        .session()
        .request_name("mecha.Test", NameFlags::DO_NOT_QUEUE);
    f.turn();
    let log = take(&mut f);
    assert!(
        log.iter()
            .any(|e| matches!(e, Ev::Reply(r) if r.call == call && r.result.is_ok()))
    );
    assert!(
        log.iter()
            .any(|e| matches!(e, Ev::Acquired(n) if n == "mecha.Test"))
    );
}

#[test]
fn daemon_helpers_reach_the_daemon() {
    let mut f = fake();
    f.owners.insert("org.bluez".into(), ":1.5".into());
    let (owner, has, names) = {
        let mut dbus = f.app.resource_mut::<DBus>();
        let mut bus = dbus.session();
        (
            bus.get_name_owner("org.bluez"),
            bus.name_has_owner("org.bluez"),
            bus.list_names(),
        )
    };
    f.turn();
    let log = take(&mut f);
    let reply = |id: CallId| {
        log.iter()
            .find_map(|e| match e {
                Ev::Reply(r) if r.call == id => Some(r.result.clone().unwrap()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(reply(owner).body::<(String,)>().unwrap().0, ":1.5");
    assert_eq!(reply(has).body::<(bool,)>().unwrap().0, true);
    assert!(
        reply(names)
            .body::<(Vec<String>,)>()
            .unwrap()
            .0
            .contains(&"org.bluez".to_string())
    );
}

#[test]
fn watch_properties_sees_one_interface() {
    let mut f = fake();
    f.owners.insert(NM.into(), ":1.30".into());
    let id = f
        .app
        .resource_mut::<DBus>()
        .session()
        .watch_properties(NM, NM_PATH, NM);
    f.turn();
    let changed = |iface: &str| {
        from(
            ":1.30",
            NM_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .with_body(&(
            iface,
            PropMap::from([("WirelessEnabled".to_string(), Value::Bool(false))]),
            Vec::<String>::new(),
        ))
    };
    f.send(changed(NM));
    f.send(changed("org.freedesktop.NetworkManager.Device"));
    f.turn();
    let s = signals(&mut f);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].matches.as_slice(), [id]);
    let (iface, props, invalidated) = PropertiesChanged::parse(&s[0].msg).unwrap();
    assert_eq!(iface.as_str(), NM);
    assert_eq!(props.get_as::<bool>("WirelessEnabled"), Some(Ok(false)));
    assert!(invalidated.is_empty());
}
