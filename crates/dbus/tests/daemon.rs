//! Against a real `dbus-daemon`, when one is installed: two connections
//! in one app, one serving and one calling, through the daemon.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use app::prelude::*;
use dbus::*;
use ring::prelude::*;

const NAME: &str = "mecha.Test";
const PATH: &str = "/mecha/Test";
const IFACE: &str = "mecha.Test";
const SERVER: BusId = BusId::Custom(0);
const CLIENT: BusId = BusId::Custom(1);

struct Daemon {
    child: Child,
    address: String,
    dir: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn daemon() -> Option<Daemon> {
    let dir = std::env::temp_dir().join(format!("mecha-dbus-daemon-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let sock = dir.join("bus");
    let config = dir.join("bus.conf");
    std::fs::write(
        &config,
        format!(
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
            sock.display()
        ),
    )
    .ok()?;
    let spawned = Command::new("dbus-daemon")
        .arg(format!("--config-file={}", config.display()))
        .arg("--nofork")
        .arg("--print-address")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(_) => {
            eprintln!("skipping: dbus-daemon is not installed");
            let _ = std::fs::remove_dir_all(&dir);
            return None;
        }
    };
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .ok()?;
    Some(Daemon {
        child,
        address: line.trim().to_owned(),
        dir,
    })
}

#[derive(Default)]
struct Log {
    replies: Vec<MethodReply>,
    signals: Vec<SignalReceived>,
}
impl Resource for Log {}

fn on_reply(app: &mut App, e: &MethodReply) {
    app.resource_mut::<Log>().replies.push(e.clone());
}
fn on_signal(app: &mut App, e: &SignalReceived) {
    app.resource_mut::<Log>().signals.push(e.clone());
}

/// The server: echo strings, hand fds back.
fn serve(app: &mut App, e: &MethodCallReceived) {
    let mut dbus = app.resource_mut::<DBus>();
    match e.call.member().as_str() {
        "Echo" => {
            let (s,): (String,) = e.call.args().unwrap();
            dbus.reply(&e.call, &(s,));
        }
        "Fd" => {
            let (fd,): (UnixFd,) = e.call.args().unwrap();
            dbus.reply(&e.call, &(fd,));
        }
        _ => {}
    }
}

/// Turn until `done`, at most five seconds. A watchdog timeout makes
/// sure a turn wakes up even if nothing arrives.
fn wait_for(app: &mut App, done: impl Fn(&Log) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    app.resource_mut::<Ring>().timeout(Duration::from_secs(5));
    while !done(app.resource::<Log>()) {
        assert!(Instant::now() < deadline, "timed out waiting on the daemon");
        Ring::turn(app);
    }
}

fn result(app: &App, id: CallId) -> Message {
    app.resource::<Log>()
        .replies
        .iter()
        .find(|r| r.call == id)
        .unwrap()
        .result
        .clone()
        .unwrap()
}

#[test]
fn a_call_a_property_a_signal_and_an_fd_through_a_real_daemon() {
    let Some(d) = daemon() else {
        return;
    };
    let mut app = App::new();
    app.add_module(RingModule::default()).add_module(
        DBusModule::new()
            .address(SERVER, &d.address)
            .address(CLIENT, &d.address),
    );
    app.init_resource::<Log>()
        .system(on_reply)
        .system(on_signal)
        .system(serve);

    let spec = InterfaceSpec::new(IFACE)
        .method("Echo", "s", "s")
        .method("Fd", "h", "h")
        .property("Count", "i", Access::Read, Value::I32(3))
        .signal("Ping", "s");
    let (named, server_name) = {
        let mut dbus = app.resource_mut::<DBus>();
        let mut s = dbus.bus(SERVER).unwrap();
        s.export(PATH, spec).unwrap();
        (
            s.request_name(NAME, NameFlags::DO_NOT_QUEUE),
            s.unique_name().clone(),
        )
    };
    wait_for(&mut app, |l| l.replies.iter().any(|r| r.call == named));

    let (echo, count, sub) = {
        let mut dbus = app.resource_mut::<DBus>();
        let mut c = dbus.bus(CLIENT).unwrap();
        let sub = c.add_match(
            MatchRule::signal()
                .sender(NAME)
                .interface(IFACE)
                .member("Ping"),
        );
        let mut o = c.object(NAME, PATH);
        (
            o.call(IFACE, "Echo", &("hello",)),
            o.get_property(IFACE, "Count"),
            sub,
        )
    };
    wait_for(&mut app, |l| {
        [echo, count]
            .iter()
            .all(|id| l.replies.iter().any(|r| r.call == *id))
    });
    assert_eq!(result(&app, echo).body::<(String,)>().unwrap().0, "hello");
    assert_eq!(
        result(&app, count).body::<(Value,)>().unwrap().0,
        Value::I32(3)
    );
    // Both buses started their serials at 1; the bus in the id keeps
    // their replies apart.
    assert_ne!(echo.bus, named.bus);

    app.resource_mut::<DBus>()
        .bus(SERVER)
        .unwrap()
        .emit_signal(PATH, IFACE, "Ping", &("pong",));
    wait_for(&mut app, |l| !l.signals.is_empty());
    {
        let s = &app.resource::<Log>().signals[0];
        assert_eq!(s.matches.as_slice(), [sub]);
        assert_eq!(s.msg.sender(), Some(&server_name));
        assert_eq!(s.msg.body::<(String,)>().unwrap().0, "pong");
    }

    let (mut reader, writer) = std::io::pipe().unwrap();
    let fd_call = app
        .resource_mut::<DBus>()
        .bus(CLIENT)
        .unwrap()
        .object(NAME, PATH)
        .call(IFACE, "Fd", &(UnixFd::new(writer.into()),));
    wait_for(&mut app, |l| l.replies.iter().any(|r| r.call == fd_call));
    let (fd,): (UnixFd,) = result(&app, fd_call).body().unwrap();
    let mut w = std::io::PipeWriter::from(fd.try_clone_owned().unwrap());
    w.write_all(b"through the bus").unwrap();
    drop(w);
    drop(fd);
    drop(app); // every other copy of the writer
    let mut got = String::new();
    reader.read_to_string(&mut got).unwrap();
    assert_eq!(got, "through the bus");
}
