//! The SASL handshake, blocking, before the socket goes to the ring:
//! `EXTERNAL` with our uid, then `NEGOTIATE_UNIX_FD`, then `BEGIN`.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

/// Authenticate on a freshly connected `stream`. `Ok(true)` if the bus
/// agreed to pass fds. Reads one byte at a time so nothing after the
/// last reply line is consumed.
pub(crate) fn authenticate(stream: &mut UnixStream, uid: u32) -> io::Result<bool> {
    let hex: String = uid
        .to_string()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
    let line = read_line(stream)?;
    if !line.starts_with("OK ") {
        return Err(io::Error::other(format!("authentication rejected: {line}")));
    }
    stream.write_all(b"NEGOTIATE_UNIX_FD\r\n")?;
    let line = read_line(stream)?;
    let fds = line == "AGREE_UNIX_FD";
    if !fds && !line.starts_with("ERROR") {
        return Err(io::Error::other(format!("unexpected reply: {line}")));
    }
    stream.write_all(b"BEGIN\r\n")?;
    Ok(fds)
}

fn read_line(stream: &mut UnixStream) -> io::Result<String> {
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the bus closed during authentication",
            ));
        }
        out.push(byte[0]);
        if out.ends_with(b"\r\n") {
            out.truncate(out.len() - 2);
            return String::from_utf8(out).map_err(io::Error::other);
        }
        if out.len() > 512 {
            return Err(io::Error::other("authentication line too long"));
        }
    }
}

/// The effective uid, from `/proc/self/status` (no unsafe `geteuid`).
pub(crate) fn effective_uid() -> io::Result<u32> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().nth(1))
        .and_then(|uid| uid.parse().ok())
        .ok_or_else(|| io::Error::other("no Uid line in /proc/self/status"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::JoinHandle;

    /// A bus side that answers each line the client ends with `\r\n`
    /// with the next of `replies`, and returns everything it read once
    /// it sees `BEGIN` or the client hangs up.
    fn server(replies: &'static [&'static str]) -> (UnixStream, JoinHandle<String>) {
        let (client, mut server) = UnixStream::pair().unwrap();
        let handle = std::thread::spawn(move || {
            let mut got = Vec::new();
            let mut byte = [0u8; 1];
            let mut replies = replies.iter();
            loop {
                if server.read(&mut byte).unwrap() == 0 {
                    break;
                }
                got.push(byte[0]);
                if got.ends_with(b"\r\n") {
                    if got.ends_with(b"BEGIN\r\n") {
                        break;
                    }
                    if let Some(r) = replies.next() {
                        server.write_all(r.as_bytes()).unwrap();
                    }
                }
            }
            String::from_utf8(got).unwrap()
        });
        (client, handle)
    }

    #[test]
    fn external_auth_with_fd_passing() {
        let (mut c, h) = server(&["OK 1234deadbeef\r\n", "AGREE_UNIX_FD\r\n"]);
        assert!(authenticate(&mut c, 1000).unwrap());
        assert_eq!(
            h.join().unwrap(),
            "\0AUTH EXTERNAL 31303030\r\nNEGOTIATE_UNIX_FD\r\nBEGIN\r\n"
        );
    }

    #[test]
    fn fd_passing_refused() {
        let (mut c, h) = server(&["OK 1234\r\n", "ERROR not here\r\n"]);
        assert!(!authenticate(&mut c, 0).unwrap());
        h.join().unwrap();
    }

    #[test]
    fn a_rejection_is_an_error() {
        let (mut c, h) = server(&["REJECTED EXTERNAL\r\n"]);
        assert!(authenticate(&mut c, 1000).is_err());
        drop(c);
        h.join().unwrap();
    }

    #[test]
    fn the_uid_is_readable() {
        assert!(effective_uid().is_ok());
    }
}
