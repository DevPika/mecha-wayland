use std::os::unix::net::UnixStream;

use app::prelude::*;
use dbus::fake::{FakeBus, UNIQUE};
use dbus::{BusId, DBus, DBusModule};

#[test]
fn install_authenticates_says_hello_and_learns_the_unique_name() {
    let mut f = FakeBus::new();
    assert!(f.auth().starts_with("\0AUTH EXTERNAL "), "{:?}", f.auth());
    assert!(f.auth().ends_with("NEGOTIATE_UNIX_FD\r\nBEGIN\r\n"));
    let hello = f.expect_call("Hello");
    assert_eq!(hello.serial(), 1);
    assert_eq!(
        hello.destination().unwrap().as_str(),
        "org.freedesktop.DBus"
    );
    let mut dbus = f.app.resource_mut::<DBus>();
    let bus = dbus.session();
    assert_eq!(bus.id(), BusId::Session);
    assert_eq!(bus.unique_name().as_str(), UNIQUE);
    assert!(bus.fds_supported());
    assert!(bus.is_connected());
}

#[test]
fn a_bus_that_refuses_fds_is_noted() {
    let mut f = FakeBus::without_fds();
    assert!(!f.app.resource_mut::<DBus>().session().fds_supported());
}

#[test]
fn an_unrequested_bus_is_none() {
    let mut f = FakeBus::new();
    assert!(f.app.resource_mut::<DBus>().bus(BusId::System).is_none());
}

#[test]
#[should_panic(expected = "the system bus was not requested")]
fn the_system_bus_without_a_request_panics() {
    let mut f = FakeBus::new();
    let _ = f.app.resource_mut::<DBus>().system();
}

#[test]
#[should_panic(expected = "DBusModule installs after RingModule")]
fn install_needs_the_ring() {
    let (a, _b) = UnixStream::pair().unwrap();
    let mut app = App::new();
    app.add_module(DBusModule::new().over(BusId::Session, a));
}
