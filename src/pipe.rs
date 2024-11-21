
use super::{wrap_errno, CrecheResult, Error, ErrorKind, Op};
use libc::{c_int, pipe};
pub use libc::{O_CLOEXEC, O_DIRECT, O_NONBLOCK};
use std::os::fd::{AsRawFd, RawFd};
use std::fmt;

bitflags::bitflags! {
    /// Flags for constructing a pipe
    pub struct O: libc::c_int {
        // Usual flags
        const CLOEXEC = libc::O_CLOEXEC;
        const DIRECT = libc::O_DIRECT;
        const NONBLOCK = libc::O_NONBLOCK;
        // strange
        const NOTIFICATION_PIPE = libc::O_EXCL;
    }
}

/// Creates a pipe. o_flags are a bitmask composed from O::* constants.
/// Returns (readfd, writefd).
pub fn make_pipe(o_flags: O) -> CrecheResult<(RawFd, RawFd)> {
    unsafe {
        let mut fd_buf: [c_int; 2] = std::mem::zeroed();
        wrap_errno(
            ErrorKind::Pipe,
            Op::Create,
            libc::pipe2(&mut fd_buf as *mut c_int, o_flags.bits()),
        )?;
    Ok((fd_buf[0], fd_buf[1]))
    }
}
