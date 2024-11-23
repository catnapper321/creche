use libc::{
    c_int, c_void, pid_t,
    syscall, SYS_clone3,
};
use std::{
    os::fd::{RawFd, AsRawFd},
    mem,
    ptr,
    fmt,
    thread::sleep,
    time::Duration,
};
use super::error::*;

/// Returns Ok(Some(pidfd)) in the parent, Ok(None) in the child process
pub fn clone_process() -> CrecheResult<Option<Pidfd>> {
    let mut fd: RawFd = Default::default(); // buffer for the pidfd
    let mut clone_args = CloneArgs::default();
    clone_args.set_flags(CLONE::FLAGS_PROCESS | CLONE::PIDFD);
    clone_args.pidfd = &mut fd as *mut _;
    let pid = unsafe {
        wrap_syscall(
            ErrorKind::Pidfd,
            Op::Create,
            libc::syscall(libc::SYS_clone3, clone_args.as_ptr(), CloneArgs::SIZE),
        )
    }?;
    if pid == 0 {
        Ok(None)
    } else {
        Ok(Some(Pidfd { pid, fd }))
    }
}

// pub fn clone3_test() -> CrecheResult<()> {
//     if let Some(pidfd) = clone_process()? {
//         // parent
//         println!("pidfd is {:?}", pidfd);
//         let status = pidfd.wait()?;
//         println!("exit status was: {:?}", status);
//     } else {
//         // child
//         println!("In child! Waiting...");
//         sleep(Duration::from_secs(1));
//         std::process::exit(2);
//     }

//     Ok(())
// }

#[repr(C)]
struct CloneArgs {
    flags: u64,
    pidfd: *mut RawFd,
    child_tid: *mut libc::pid_t,
    parent_tid: *mut libc::pid_t,
    exit_signal: u64,
    stack: *mut u8, // ptr to lowest byte of the stack
    stack_size: usize,
    tls: *mut u8, // TODO: ptr to TLS info struct
    set_tid: *mut pid_t, // ptr to pid_t array
    set_tid_size: usize, // elements in the set_tid array
    cgroup: u64, // fd for target cgroup of the child
}
impl CloneArgs {
    pub const SIZE: usize = core::mem::size_of::<Self>();
    pub fn as_ptr(&self) -> *const Self {
        self as *const _
    }
    pub fn set_flags(&mut self, flags: CLONE) {
        self.flags = flags.bits() as u64;
    }
    pub fn set_sigchld(&mut self, value: bool) {
        if value { 
            self.exit_signal = libc::SIGCHLD as u64;
        } else {
            self.exit_signal = 0;
        }
    }
}
impl Default for CloneArgs {
    fn default() -> Self {
        Self {
            flags: 0,
            pidfd: ptr::null_mut(),
            child_tid: ptr::null_mut(),
            parent_tid: ptr::null_mut(),
            exit_signal: libc::SIGCHLD as u64,
            stack: ptr::null_mut(),
            stack_size: 0,
            tls: ptr::null_mut(),
            set_tid: ptr::null_mut(),
            set_tid_size: 0,
            cgroup: 0,
        }
    }
}
impl CloneArgs {
    /// Default termination signal is SIGCHLD (17)
    pub fn set_exit_signal(&mut self, signal: c_int) {
        self.exit_signal = signal as u64;
    }
}

/// Assuming the pidfd was not made with NONBLOCK and clone3 was
/// configurated to signal on task termination, this function waits for and
/// collects the exit status of the child process
pub fn pidfd_wait(pidfd: &Pidfd) -> CrecheResult<ChildStatus> {
    unsafe {
        let mut siginfo: WaitSiginfoT = mem::zeroed();
        let sig_ptr: *mut libc::siginfo_t = mem::transmute(&mut siginfo);
        _ = wrap_errno(
            ErrorKind::Pidfd, Op::Wait,
            libc::waitid(libc::P_PIDFD, pidfd.as_raw_fd() as u32, sig_ptr, libc::WEXITED)
        )?;
        Ok(ChildStatus::from(siginfo))
    }
}

/// Struct for receiving exit data from `libc::waitid()`
#[repr(C)]
struct WaitSiginfoT {
    signo: c_int, // AFAIK always set to SIGCHLD
    _ignore1: c_int,
    code: c_int, // reason for signal
    _ignore2: [u8; 12],
    status: c_int, // value provided to exit() 
    _ignore3: [u8; 100]
}
impl fmt::Debug for WaitSiginfoT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("siginfo_t {")?;
        f.write_fmt(format_args!("code: {:?}, status: {:?}", self.code, self.status))?;
        f.write_str("}")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChildStatusKind {
    /// Child process exited normally
    Exited,
    /// The child was killed by a signal
    Killed,
    /// The child dumped core
    Dumped,
    /// The child process was stopped by a signal
    Stopped,
    /// Traced child process has trapped
    Trapped,
    /// The child process received SIGCONT
    Continued,
    Unknown
}
#[derive(Debug, Clone, Copy)]
pub struct ChildStatus {
    kind: ChildStatusKind,
    status: c_int,
}
impl From<WaitSiginfoT> for ChildStatus {
    fn from(value: WaitSiginfoT) -> Self {
        let kind = match value.code {
            libc::CLD_EXITED => ChildStatusKind::Exited,
            libc::CLD_KILLED => ChildStatusKind::Killed,
            libc::CLD_DUMPED => ChildStatusKind::Dumped,
            libc::CLD_STOPPED => ChildStatusKind::Stopped,
            libc::CLD_TRAPPED => ChildStatusKind::Trapped,
            libc::CLD_CONTINUED => ChildStatusKind::Continued,
            _ => ChildStatusKind::Unknown,
        };
        ChildStatus { kind, status: value.status }
    }
}


bitflags::bitflags! {
    pub struct CLONE: c_int {
        const CHILD_CLEARTID = libc::CLONE_CHILD_CLEARTID;
        const CHILD_SETTID = libc::CLONE_CHILD_SETTID;
        const CLEAR_SIGHAND = libc::CLONE_CLEAR_SIGHAND;
        const DETACHED = libc::CLONE_DETACHED;
        const FILES = libc::CLONE_FILES;
        const FS = libc::CLONE_FS;
        const INTO_CGROUP = libc::CLONE_INTO_CGROUP;
        const IO = libc::CLONE_IO;
        // this was the CLONE_STOPPED bit on old kernels
        const NEWCGROUP = libc::CLONE_NEWCGROUP;
        const NEWIPC = libc::CLONE_NEWIPC;
        const NEWNET = libc::CLONE_NEWNET;
        const NEWNS = libc::CLONE_NEWNS;
        const NEWPID = libc::CLONE_NEWPID;
        const NEWUSER = libc::CLONE_NEWUSER;
        const NEWUTS = libc::CLONE_NEWUTS;
        const PARENT = libc::CLONE_PARENT;
        const PARENT_SETTID = libc::CLONE_PARENT_SETTID;
        // no longer used on modern kernels
        // const CLONE_PID = libc::CLONE_PID;
        // a long time ago, in a kernel far, far away, this was CLONE_PID
        const PIDFD = libc::CLONE_PIDFD;
        const PTRACE = libc::CLONE_PTRACE;
        const SETTLS = libc::CLONE_SETTLS;
        const SIGHAND = libc::CLONE_SIGHAND;
        const SYSVSEM = libc::CLONE_SYSVSEM;
        const THREAD = libc::CLONE_THREAD;
        const UNTRACED = libc::CLONE_UNTRACED;
        const VFORK = libc::CLONE_VFORK;
        const VM = libc::CLONE_VM;
        // convenience const for clone3 thread spawning
        const FLAGS_THREAD = libc::CLONE_VM | libc::CLONE_FS | libc::CLONE_FILES | libc::CLONE_SYSVSEM | libc::CLONE_SIGHAND | libc::CLONE_THREAD | libc::CLONE_SETTLS | libc::CLONE_PARENT_SETTID | libc::CLONE_CHILD_CLEARTID;
        const FLAGS_PROCESS = libc::CLONE_CHILD_CLEARTID | libc::CLONE_CHILD_SETTID;
    }
}

#[derive(Debug)]
pub struct Pidfd {
    fd: RawFd,
    pid: i64, 
}
impl AsRawFd for Pidfd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}
impl AsRawFd for &Pidfd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}
impl Drop for Pidfd {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}
impl Default for Pidfd {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}
impl Pidfd {
    pub fn wait(&self) -> CrecheResult<ChildStatus> {
        pidfd_wait(&self)
    }
    pub fn pid(&self) -> i64 {
        self.pid
    }
    pub fn signal(&self, signo: i32) -> CrecheResult<()> {
        let ptr: *mut libc::siginfo_t = ptr::null_mut();
        unsafe {
            _ = wrap_syscall(
                ErrorKind::Pidfd,
                Op::Signal,
                syscall(libc::SYS_pidfd_send_signal,
                    self.fd,
                    signo,
                    ptr,
                    0)
            );
        }
        Ok(())
    }
}
