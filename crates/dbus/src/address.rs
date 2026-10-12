//! Bus addresses: `unix:path=…` and `unix:abstract=…`, `;`-separated
//! alternatives tried in order, `%xx` escapes. Other transports are
//! skipped.

use std::env;
use std::ffi::OsString;
use std::io;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::path::PathBuf;

use crate::auth;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Addr {
    Path(PathBuf),
    Abstract(Vec<u8>),
}

pub(crate) fn parse(s: &str) -> Result<Vec<Addr>, String> {
    let mut out = Vec::new();
    for entry in s.split(';').filter(|e| !e.is_empty()) {
        let Some((transport, params)) = entry.split_once(':') else {
            return Err(format!("no transport in {entry:?}"));
        };
        if transport != "unix" {
            continue;
        }
        let mut addr = None;
        for kv in params.split(',') {
            let Some((k, v)) = kv.split_once('=') else {
                return Err(format!("not key=value: {kv:?}"));
            };
            let v = unescape(v)?;
            match k {
                "path" => addr = Some(Addr::Path(PathBuf::from(OsString::from_vec(v)))),
                "abstract" => addr = Some(Addr::Abstract(v)),
                _ => {}
            }
        }
        out.push(addr.ok_or_else(|| format!("no path or abstract in {entry:?}"))?);
    }
    if out.is_empty() {
        return Err(format!("no usable unix address in {s:?}"));
    }
    Ok(out)
}

fn unescape(v: &str) -> Result<Vec<u8>, String> {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let byte = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| format!("bad escape in {v:?}"))?;
            out.push(byte);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

/// Connect to the first alternative that accepts; the last error if none.
pub(crate) fn connect(s: &str) -> io::Result<UnixStream> {
    let addrs = parse(s).map_err(io::Error::other)?;
    let mut last = io::Error::other("no address");
    for a in addrs {
        let r = match &a {
            Addr::Path(p) => UnixStream::connect(p),
            Addr::Abstract(name) => {
                SocketAddr::from_abstract_name(name).and_then(|sa| UnixStream::connect_addr(&sa))
            }
        };
        match r {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// `DBUS_SESSION_BUS_ADDRESS`, else `$XDG_RUNTIME_DIR/bus`.
pub(crate) fn session() -> String {
    env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
        let dir = env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", auth::effective_uid().unwrap_or(0)));
        format!("unix:path={dir}/bus")
    })
}

/// `DBUS_SYSTEM_BUS_ADDRESS`, else the standard system socket.
pub(crate) fn system() -> String {
    env::var("DBUS_SYSTEM_BUS_ADDRESS")
        .unwrap_or_else(|_| "unix:path=/run/dbus/system_bus_socket".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn parses_paths_abstract_names_and_alternatives() {
        assert_eq!(
            parse("unix:path=/run/user/1000/bus").unwrap(),
            [Addr::Path("/run/user/1000/bus".into())]
        );
        assert_eq!(
            parse("unix:abstract=/tmp/dbus-x,guid=abc").unwrap(),
            [Addr::Abstract(b"/tmp/dbus-x".to_vec())]
        );
        assert_eq!(
            parse("tcp:host=x,port=1;unix:path=/a%20b").unwrap(),
            [Addr::Path("/a b".into())]
        );
        assert!(parse("tcp:host=x").is_err());
        assert!(parse("").is_err());
        assert!(parse("unix:path=/a%2").is_err());
        assert!(parse("unix:guid=x").is_err());
    }

    #[test]
    fn connects_to_a_listening_path() {
        let dir = std::env::temp_dir().join(format!("mecha-dbus-addr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("bus");
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();
        let stream = connect(&format!("unix:path={}", sock.display())).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        drop(stream);
        assert!(connect("unix:path=/nonexistent/mecha/dbus/socket").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
