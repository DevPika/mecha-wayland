//! A file descriptor carried in a message.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::rc::Rc;

/// An fd in a message (`h`). Shared, because signals reach systems by
/// reference: clone it to keep it, [`UnixFd::try_clone_owned`] to own a
/// copy. Two `UnixFd`s are equal when they share the same fd.
#[derive(Debug, Clone)]
pub struct UnixFd(Rc<OwnedFd>);

impl UnixFd {
    pub fn new(fd: OwnedFd) -> Self {
        UnixFd(Rc::new(fd))
    }

    /// A `dup` of the fd, owned by the caller.
    pub fn try_clone_owned(&self) -> io::Result<OwnedFd> {
        self.0.try_clone()
    }
}

impl AsFd for UnixFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl From<OwnedFd> for UnixFd {
    fn from(fd: OwnedFd) -> Self {
        UnixFd::new(fd)
    }
}

impl PartialEq for UnixFd {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
