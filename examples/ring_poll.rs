//! A headless example of socket readiness: `cargo run --example ring_poll`.
//! Polling reports readable data without consuming it. No threads or timers.

use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

use mecha_wayland::prelude::Ring;

const MESSAGE: &[u8] = b"hello from Ring";

fn main() -> std::io::Result<()> {
    let (mut reader, mut writer) = UnixStream::pair()?;
    reader.set_nonblocking(true)?; // gives us instant errors
    let mut ring = Ring::new(8);

    // Queue the poll before sending anything to the initially empty socket.
    let poll = ring.poll(reader.as_fd(), libc::POLLIN as u32);
    writer.write_all(MESSAGE)?;

    // Submit and wait. The poll is the only queued operation.
    let event = ring.wait()[0];
    assert_eq!(event.token, poll);
    assert!(event.result >= 0, "Poll failed: {event:?}");
    assert_ne!(event.result & libc::POLLIN as i32, 0);
    println!("Poll: socket is readable (flags: {:#x})", event.result);

    let completed = ring.finish(poll).expect("poll completed");
    assert!(completed.buf.is_empty() && completed.fds.is_empty());

    // Read separately, like libpulse would. The poll left the bytes untouched.
    let mut received = vec![0; MESSAGE.len()];
    reader.read_exact(&mut received)?;
    assert_eq!(received, MESSAGE);
    println!("Read after polling: {}", String::from_utf8_lossy(&received));

    Ok(())
}
