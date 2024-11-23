use libc::c_int;
use std::{
    os::fd::{AsRawFd, RawFd},
    io,
    fmt,
    ffi::CStr,
};

pub type CrecheResult<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, PartialEq)]
pub enum Error {
    EOF,
    WouldBlock,
    Timeout,
    Errno {
        kind: ErrorKind, 
        op: Op,
        errno: c_int
    },
}
impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EOF => f.write_str("EOF"),
            Error::WouldBlock => f.write_str("WouldBlock"),
            Error::Timeout => f.write_str("Timeout"),
            Error::Errno { kind, op, errno } => {
                let e_name = unsafe {
                    let ptr = libc::strerror(*errno);
                    CStr::from_ptr(ptr)
                };
                f.write_fmt(format_args!("Error: errno {} ({:?}), {:?}::{:?}", errno, e_name, kind, op))
            }
        }
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        io::Error::new(io::ErrorKind::Other, value)
    }
}
impl<T> From<Error> for std::result::Result<T, Error> {
    fn from(value: Error) -> Self {
        std::result::Result::Err(value)
    }
}
impl Error {
    /// Returns a new Errno error
    pub fn new(kind: ErrorKind, operation: Op, errno: c_int) -> Self {
        Self::Errno { kind, op: operation, errno }
    }
    pub fn would_block(&self) -> bool {
        matches!(self, Self::WouldBlock)
    }
    // alias for would_block
    pub fn eagain(&self) -> bool {
        self.would_block()
    }
    pub fn eof(&self) -> bool {
        matches!(self, Self::EOF)
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    Pidfd,
    Pipe,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Create,
    Read,
    Write,
    Wait,
    Signal
}

#[inline(always)]
pub fn errno() -> c_int {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[inline(always)]
pub fn wrap_errno(kind: ErrorKind, operation: Op, value: c_int) -> CrecheResult<c_int> {
    if value == -1 {
        Err(Error::new(
            kind,
            operation,
            errno(),
        ))
    } else {
        Ok(value)
    }
}

// TODO: consider using a macro
#[inline(always)]
pub fn wrap_syscall(kind: ErrorKind, operation: Op, value: i64) -> CrecheResult<i64> {
    if value == -1 {
        Err(Error::new(
            kind,
            operation,
            errno(),
        ))
    } else {
        Ok(value)
    }
}

pub fn wrap_read<T>(fd: RawFd, kind: ErrorKind, buf: *mut T, count: libc::size_t) -> CrecheResult<isize> {
    const OPR: Op = Op::Read;
    // TODO: check for null pointer?
    let n = unsafe {
        libc::read(fd, buf as *mut _, count)
    };
    if n == -1 {
        let e = errno();
        if e == libc::EAGAIN || e == libc::EWOULDBLOCK {
            return Error::WouldBlock.into()
        } else {
            return Error::new(kind, OPR, e).into();
        }
    }
    if n == 0 {
        return Error::EOF.into();
    }
    Ok(n)
}

pub fn wrap_write<T>(fd: RawFd, kind: ErrorKind, buf: *const T, count: libc::size_t) -> CrecheResult<isize> {
    const OPR: Op = Op::Write;
    let n = unsafe {
        libc::write(fd, buf as *const _, count)
    };
    if n == -1 {
        let e = errno();
        if e == libc::EAGAIN || e == libc::EWOULDBLOCK {
            return Error::WouldBlock.into()
        } else {
            return Error::new(kind, OPR, e).into();
        }
    }
    if n == 0 {
        return Error::EOF.into();
    }
    Ok(n)
}
