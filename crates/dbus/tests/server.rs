use app::prelude::*;
use dbus::fake::{FakeBus, UNIQUE};
use dbus::*;

const PATH: &str = "/mecha/Test";
const IFACE: &str = "mecha.Test";

/// Whether `on_call` keeps the calls it sees.
#[derive(Default)]
struct Keep(bool);
impl Resource for Keep {}

#[derive(Default)]
struct Kept(Vec<(ExportId, IncomingCall)>);
impl Resource for Kept {}

fn on_call(app: &mut App, e: &MethodCallReceived) {
    if app.resource::<Keep>().0 {
        app.resource_mut::<Kept>()
            .0
            .push((e.export, e.call.clone()));
    }
}

fn spec() -> InterfaceSpec {
    InterfaceSpec::new(IFACE)
        .method("Echo", "s", "s")
        .method("Nothing", "", "")
}

fn fake(keep: bool) -> (FakeBus, ExportId) {
    let mut f = FakeBus::new();
    f.app.insert_resource(Keep(keep));
    f.app.init_resource::<Kept>().system(on_call);
    let export = f
        .app
        .resource_mut::<DBus>()
        .session()
        .export(PATH, spec())
        .unwrap();
    f.received();
    (f, export)
}

fn call(path: &str, iface: Option<&str>, member: &str) -> Message {
    let m = Message::method_call(
        ObjectPath::new(path).unwrap(),
        MemberName::new(member).unwrap(),
    )
    .with_destination(UNIQUE)
    .with_sender(":1.9");
    match iface {
        Some(i) => m.with_interface(InterfaceName::new(i).unwrap()),
        None => m,
    }
}

fn answers(f: &mut FakeBus) -> Vec<Message> {
    f.received()
        .into_iter()
        .filter(|m| matches!(m.kind(), MessageKind::MethodReturn | MessageKind::Error))
        .collect()
}

#[test]
fn a_call_can_be_answered_later() {
    let (mut f, export) = fake(true);
    let serial = f.send(call(PATH, Some(IFACE), "Echo").with_body(&("hi",)));
    f.turn();
    assert!(answers(&mut f).is_empty(), "nothing until the app answers");
    let (got, kept) = f.app.resource_mut::<Kept>().0.pop().unwrap();
    assert_eq!(got, export);
    assert_eq!(kept.member().as_str(), "Echo");
    assert_eq!(kept.sender().unwrap().as_str(), ":1.9");
    assert_eq!(kept.args::<(String,)>().unwrap().0, "hi");
    f.app.resource_mut::<DBus>().reply(&kept, &("hi back",));
    assert!(kept.is_answered());
    f.turn();
    let a = answers(&mut f);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind(), MessageKind::MethodReturn);
    assert_eq!(a[0].reply_serial(), Some(serial));
    assert_eq!(a[0].destination().unwrap().as_str(), ":1.9");
    assert_eq!(a[0].body::<(String,)>().unwrap().0, "hi back");
}

#[test]
fn a_call_no_system_keeps_gets_no_reply() {
    let (mut f, _) = fake(false);
    let serial = f.send(call(PATH, Some(IFACE), "Echo").with_body(&("hi",)));
    f.turn();
    let a = answers(&mut f);
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].error_name().unwrap().as_str(),
        "org.freedesktop.DBus.Error.NoReply"
    );
    assert_eq!(a[0].reply_serial(), Some(serial));
}

#[test]
fn answering_twice_sends_one_answer() {
    let (mut f, _) = fake(true);
    f.send(call(PATH, Some(IFACE), "Nothing"));
    f.turn();
    let (_, kept) = f.app.resource_mut::<Kept>().0.pop().unwrap();
    f.app.resource_mut::<DBus>().reply(&kept, &());
    f.app
        .resource_mut::<DBus>()
        .reply_error(&kept, "org.freedesktop.DBus.Error.Failed", "late");
    drop(kept);
    f.turn();
    let a = answers(&mut f);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind(), MessageKind::MethodReturn);
}

#[test]
fn a_call_that_wants_no_reply_gets_none() {
    let (mut f, _) = fake(false);
    f.send(call(PATH, Some(IFACE), "Nothing").with_flags(Flags::NO_REPLY_EXPECTED));
    f.turn();
    assert!(answers(&mut f).is_empty());
}

#[test]
fn bad_calls_are_answered_before_any_system_sees_them() {
    let (mut f, _) = fake(true);
    let cases = [
        (
            call("/nowhere", Some(IFACE), "Echo").with_body(&("x",)),
            "UnknownObject",
        ),
        (
            call(PATH, Some("other.Iface"), "Echo").with_body(&("x",)),
            "UnknownInterface",
        ),
        (call(PATH, Some(IFACE), "Nope"), "UnknownMethod"),
        (
            call(PATH, Some(IFACE), "Echo").with_body(&(1u32,)),
            "InvalidArgs",
        ),
        (call(PATH, None, "Nope"), "UnknownMethod"),
        (call("/nowhere", None, "Nope"), "UnknownObject"),
    ];
    for (msg, err) in cases {
        let serial = f.send(msg);
        f.turn();
        let a = answers(&mut f);
        assert_eq!(a.len(), 1, "{err}");
        assert_eq!(
            a[0].error_name().unwrap().as_str(),
            format!("org.freedesktop.DBus.Error.{err}")
        );
        assert_eq!(a[0].reply_serial(), Some(serial));
    }
    assert!(
        f.app.resource::<Kept>().0.is_empty(),
        "no call reached a system"
    );
}

#[test]
fn a_call_without_an_interface_finds_its_method() {
    let (mut f, export) = fake(true);
    f.send(call(PATH, None, "Echo").with_body(&("x",)));
    f.turn();
    assert_eq!(f.app.resource::<Kept>().0.len(), 1);
    assert_eq!(f.app.resource::<Kept>().0[0].0, export);
}

#[test]
fn export_refuses_duplicates_and_standard_interfaces() {
    let (mut f, export) = fake(true);
    let mut dbus = f.app.resource_mut::<DBus>();
    let mut bus = dbus.session();
    assert_eq!(bus.export(PATH, spec()), Err(ExportError::AlreadyExported));
    assert_eq!(
        bus.export(PATH, InterfaceSpec::new("org.freedesktop.DBus.Properties")),
        Err(ExportError::Reserved)
    );
    bus.unexport(export);
    assert!(bus.export(PATH, spec()).is_ok());
}

#[test]
fn an_unexported_object_is_unknown() {
    let (mut f, export) = fake(true);
    f.app.resource_mut::<DBus>().session().unexport(export);
    f.send(call(PATH, Some(IFACE), "Nothing"));
    f.turn();
    let a = answers(&mut f);
    assert_eq!(
        a[0].error_name().unwrap().as_str(),
        "org.freedesktop.DBus.Error.UnknownObject"
    );
}

#[test]
fn signals_go_out() {
    let (mut f, _) = fake(false);
    f.app
        .resource_mut::<DBus>()
        .session()
        .emit_signal(PATH, IFACE, "Changed", &("x",));
    f.app
        .resource_mut::<DBus>()
        .session()
        .emit_signal_to(":1.9", PATH, IFACE, "Changed", &("y",));
    f.turn();
    let a = f.expect(MessageKind::Signal);
    assert_eq!(a.path().unwrap().as_str(), PATH);
    assert_eq!(a.interface().unwrap().as_str(), IFACE);
    assert_eq!(a.member().unwrap().as_str(), "Changed");
    assert!(a.destination().is_none());
    assert_eq!(a.body::<(String,)>().unwrap().0, "x");
    let b = f.expect(MessageKind::Signal);
    assert_eq!(b.destination().unwrap().as_str(), ":1.9");
}
