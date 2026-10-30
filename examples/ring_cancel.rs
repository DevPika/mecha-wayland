//! Cancel a long timeout: `cargo run --example ring_cancel`.
//! The timeout and its cancellation each produce their own completion.

use std::time::Duration;

use mecha_wayland::prelude::Ring;

fn main() {
    let mut ring = Ring::new(8);

    // This would take 60 seconds to finish if we didn't cancel it.
    let timeout = ring.timeout(Duration::from_secs(60));
    let cancel = ring.cancel(timeout);

    // Completions may arrive in either order. Finish BOTH operations.
    let mut remaining = 2;
    while remaining > 0 {
        for event in ring.wait() {
            if event.token == cancel {
                assert_eq!(event.result, 0, "cancellation failed");
                println!("Cancellation completed");
            } else if event.token == timeout {
                assert_eq!(event.result, -libc::ECANCELED);
                println!("Timeout completed with ECANCELED");
            } else {
                panic!("unexpected completion: {event:?}");
            }

            let done = ring.finish(event.token).expect("operation completed");
            assert!(done.buf.is_empty() && done.fds.is_empty());
            remaining -= 1;
        }
    }
}
