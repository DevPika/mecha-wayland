use std::collections::BTreeMap;

use app::prelude::*;
use dbus::fake::{FakeBus, UNIQUE};
use dbus::*;

const PATH: &str = "/mecha/Test";
const IFACE: &str = "mecha.Test";
const PROPS: &str = "org.freedesktop.DBus.Properties";

#[derive(Default)]
struct Sets(Vec<PropertySetRequested>);
impl Resource for Sets {}

fn on_set(app: &mut App, e: &PropertySetRequested) {
    app.resource_mut::<Sets>().0.push(e.clone());
}

fn spec() -> InterfaceSpec {
    InterfaceSpec::new(IFACE)
        .method("Echo", "s", "s")
        .property("Count", "i", Access::Read, Value::I32(3))
        .property("Name", "s", Access::ReadWrite, Value::Str("a".into()))
        .signal("Changed", "s")
}

fn fake() -> (FakeBus, ExportId) {
    let mut f = FakeBus::new();
    f.app.init_resource::<Sets>().system(on_set);
    let export = f
        .app
        .resource_mut::<DBus>()
        .session()
        .export(PATH, spec())
        .unwrap();
    f.received();
    (f, export)
}

/// Send a call to the app and return its answer.
fn ask<B: Body>(f: &mut FakeBus, path: &str, iface: &str, member: &str, body: &B) -> Message {
    let serial = f.send(
        Message::method_call(
            ObjectPath::new(path).unwrap(),
            MemberName::new(member).unwrap(),
        )
        .with_destination(UNIQUE)
        .with_sender(":1.9")
        .with_interface(InterfaceName::new(iface).unwrap())
        .with_body(body),
    );
    f.turn();
    f.received()
        .into_iter()
        .find(|m| m.reply_serial() == Some(serial))
        .expect("an answer")
}

fn error_name(m: &Message) -> &str {
    m.error_name().map(|e| e.as_str()).unwrap_or("none")
}

#[test]
fn introspection_lists_children_and_interfaces() {
    let (mut f, _) = fake();
    let root = ask(
        &mut f,
        "/",
        "org.freedesktop.DBus.Introspectable",
        "Introspect",
        &(),
    );
    let (xml,): (String,) = root.body().unwrap();
    assert!(xml.contains("<node name=\"mecha\"/>"), "{xml}");
    assert!(!xml.contains(PROPS), "nothing is exported at /");
    let obj = ask(
        &mut f,
        PATH,
        "org.freedesktop.DBus.Introspectable",
        "Introspect",
        &(),
    );
    let (xml,): (String,) = obj.body().unwrap();
    assert!(xml.contains("<interface name=\"mecha.Test\">"), "{xml}");
    assert!(
        xml.contains("<property name=\"Count\" type=\"i\" access=\"read\"/>"),
        "{xml}"
    );
    assert!(xml.contains(PROPS));
}

#[test]
fn peer_answers() {
    let (mut f, _) = fake();
    let ping = ask(&mut f, PATH, "org.freedesktop.DBus.Peer", "Ping", &());
    assert_eq!(ping.kind(), MessageKind::MethodReturn);
    let id = ask(
        &mut f,
        "/",
        "org.freedesktop.DBus.Peer",
        "GetMachineId",
        &(),
    );
    if std::path::Path::new("/etc/machine-id").exists() {
        assert_eq!(id.body::<(String,)>().unwrap().0.len(), 32);
    } else {
        assert_eq!(error_name(&id), "org.freedesktop.DBus.Error.Failed");
    }
}

#[test]
fn get_and_get_all_read_the_store() {
    let (mut f, _) = fake();
    let get = ask(&mut f, PATH, PROPS, "Get", &(IFACE, "Count"));
    assert_eq!(get.body::<(Value,)>().unwrap().0, Value::I32(3));
    let all = ask(&mut f, PATH, PROPS, "GetAll", &(IFACE,));
    let (props,): (PropMap,) = all.body().unwrap();
    assert_eq!(props.get_as::<i32>("Count"), Some(Ok(3)));
    assert_eq!(props.get_as::<String>("Name"), Some(Ok("a".to_string())));
}

#[test]
fn a_set_is_the_apps_to_accept() {
    let (mut f, export) = fake();
    let serial = f.send(
        Message::method_call(PATH, "Set")
            .with_destination(UNIQUE)
            .with_sender(":1.9")
            .with_interface(PROPS)
            .with_body(&(IFACE, "Name", Value::Str("b".into()))),
    );
    f.turn();
    assert!(
        f.received()
            .iter()
            .all(|m| m.reply_serial() != Some(serial)),
        "not yet answered"
    );
    let req = f.app.resource_mut::<Sets>().0.pop().unwrap();
    assert_eq!(req.export, export);
    assert_eq!(req.name, "Name");
    assert_eq!(req.value, Value::Str("b".into()));
    {
        let mut dbus = f.app.resource_mut::<DBus>();
        dbus.session()
            .set_property_value(req.export, "Name", req.value.clone())
            .unwrap();
        dbus.reply(&req.call, &());
    }
    drop(req);
    f.turn();
    let got = f.received();
    assert!(
        got.iter()
            .any(|m| m.reply_serial() == Some(serial) && m.kind() == MessageKind::MethodReturn)
    );
    let changed = got
        .iter()
        .find(|m| {
            m.member()
                .is_some_and(|x| x.as_str() == "PropertiesChanged")
        })
        .unwrap();
    let (iface, props, invalidated) = PropertiesChanged::parse(changed).unwrap();
    assert_eq!(iface.as_str(), IFACE);
    assert_eq!(props.get_as::<String>("Name"), Some(Ok("b".to_string())));
    assert!(invalidated.is_empty());
}

#[test]
fn bad_property_calls_are_answered_by_the_library() {
    let (mut f, _) = fake();
    let cases: Vec<(Message, &str)> = vec![
        (
            ask(&mut f, PATH, PROPS, "Set", &(IFACE, "Count", Value::I32(9))),
            "PropertyReadOnly",
        ),
        (
            ask(&mut f, PATH, PROPS, "Set", &(IFACE, "Nope", Value::I32(9))),
            "UnknownProperty",
        ),
        (
            ask(&mut f, PATH, PROPS, "Set", &(IFACE, "Name", Value::U32(9))),
            "InvalidArgs",
        ),
        (
            ask(&mut f, PATH, PROPS, "Get", &("other.Iface", "Count")),
            "UnknownInterface",
        ),
        (ask(&mut f, PATH, PROPS, "Get", &(IFACE,)), "InvalidArgs"),
        (ask(&mut f, PATH, PROPS, "Frobnicate", &()), "UnknownMethod"),
    ];
    for (answer, err) in cases {
        assert_eq!(
            error_name(&answer),
            format!("org.freedesktop.DBus.Error.{err}")
        );
    }
    assert!(f.app.resource::<Sets>().0.is_empty());
}

#[test]
fn changes_in_one_turn_are_one_signal() {
    let (mut f, export) = fake();
    {
        let mut dbus = f.app.resource_mut::<DBus>();
        let mut bus = dbus.session();
        bus.set_property_value(export, "Count", Value::I32(4))
            .unwrap();
        bus.set_property_value(export, "Count", Value::I32(5))
            .unwrap();
        bus.set_property_value(export, "Name", Value::Str("z".into()))
            .unwrap();
    }
    f.turn();
    let signals: Vec<Message> = f
        .received()
        .into_iter()
        .filter(|m| m.kind() == MessageKind::Signal)
        .collect();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].path().unwrap().as_str(), PATH);
    let (_, props, _) = PropertiesChanged::parse(&signals[0]).unwrap();
    let props: BTreeMap<_, _> = props.into_iter().collect();
    assert_eq!(
        props,
        BTreeMap::from([
            ("Count".to_string(), Value::I32(5)),
            ("Name".to_string(), Value::Str("z".into())),
        ])
    );
}

#[test]
fn an_unchanged_value_emits_nothing() {
    let (mut f, export) = fake();
    f.app
        .resource_mut::<DBus>()
        .session()
        .set_property_value(export, "Count", Value::I32(3))
        .unwrap();
    f.turn();
    assert!(f.received().iter().all(|m| m.kind() != MessageKind::Signal));
}

#[test]
fn set_property_value_checks_its_arguments() {
    let (mut f, export) = fake();
    let mut dbus = f.app.resource_mut::<DBus>();
    let mut bus = dbus.session();
    assert_eq!(
        bus.set_property_value(export, "Nope", Value::I32(1)),
        Err(PropertyError::UnknownProperty)
    );
    assert_eq!(
        bus.set_property_value(export, "Count", Value::Str("x".into())),
        Err(PropertyError::Signature {
            expected: Signature::from("i"),
            found: Signature::from("s"),
        })
    );
    bus.unexport(export);
    assert_eq!(
        bus.set_property_value(export, "Count", Value::I32(1)),
        Err(PropertyError::UnknownExport)
    );
}
