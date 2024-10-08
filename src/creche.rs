/// Configure child processes
use super::*;
use nix::unistd::{execvp, fork, ForkResult, Pid};
pub use nix::{errno::Errno, sys::signal::Signal, sys::wait::WaitStatus};
use std::{
    ffi::{CString, OsString},
    fs::File,
    ops::Deref,
    os::fd::{AsRawFd, OwnedFd, RawFd},
    sync::{Arc, Weak},
};
use utils::Argument;

/// Struct for configuration of a child process. File descriptors are
/// configured by calling [`Self::config_io()`] with values obtained
/// from the [`ioconfig`] module. Environment is configured via
/// [`Self::set_env()`] with a value obtained from
/// [`envconfig::EnvironmentBuilder`].
///
/// Example of piping data to a child process:
/// ```
/// # use creche::*;
/// # use std::io::Write;
/// let mut cmd = creche::ChildBuilder::new("tr");
/// cmd.arg("[:lower:]").arg("[:upper:]");
/// let fd = cmd.pipe_to_stdin();
/// let child = cmd.spawn();
///
/// // write test data
/// let mut f = std::fs::File::from(fd);
/// writeln!(f, "this is a test message");
/// drop(f);
///
/// println!("child exit: {:?}", child.wait());
/// ```
/// Example of reading data from a child process:
/// ```
/// # use creche::*;
/// # fn example() -> Result<(), std::io::Error> {
/// // configure the child process
/// let mut cmd = ChildBuilder::new("ls");
/// let pipe = cmd.pipe_from_stdout();
/// cmd.redirect21(); // redirect stderr to stdout
///
/// // run it
/// let mut child = cmd.spawn();
///
/// // read stdout from the child process
/// let f = std::fs::File::from(pipe);
/// let output = std::io::read_to_string(f)?;
///
/// println!("output is: {:?}", output);
/// println!("exit status: {:?}", child.wait());
/// # Ok(())
/// # }
/// ```
pub struct ChildBuilder {
    bin: CString,
    args: Vec<CString>,
    io_configs: Vec<Box<dyn ioconfig::ConfigureIO>>,
    devnull: Option<File>,
    // list of RawFd to close after the child forks -- basically any fd
    // that needs closing that does not have CLOEXEC set on it like the
    // /dev/null file descriptor.
    fds_to_close: Vec<RawFd>,
    env: Option<Vec<CString>>,
    chdir: Option<OsString>,
    chroot: Option<OsString>,
}
impl ChildBuilder {
    pub fn new(path: impl Into<Argument>) -> Self {
        let p: Argument = path.into();
        Self {
            bin: p.deref().to_owned(),
            args: vec![p.into_c_string()], // first arg is always the executable name
            io_configs: Vec::new(),
            devnull: None,
            fds_to_close: Vec::new(),
            env: None,
            chdir: None,
            chroot: None,
        }
    }
    pub fn arg(&mut self, arg: impl Into<Argument>) -> &mut Self {
        let x = arg.into();
        self.args.push(x.into_c_string());
        self
    }
    /// Accepts an [`ioconfig::IOConfig`] (a boxed trait object) that configures the file descriptors of the child process.
    pub fn config_io(&mut self, io_mut: ioconfig::IOConfig) -> &mut Self {
        self.io_configs.push(io_mut);
        self
    }
    /// Accepts an [`envconfig::Environment`], generated by a
    /// [`envconfig::EnvironmentBuilder`]. Sets up the environment for
    /// the child process.
    ///
    /// Note that this method only sets the environment on the first
    /// call to it; subsequent `.set_env()` calls will silently drop
    /// the argument. This behavior is useful when the child is being
    /// used in a pipeline builder, in the case where the child needs
    /// a different environment than the one set for the pipeline.
    pub fn set_env(&mut self, env: envconfig::Environment) -> &mut Self {
        if self.env.is_none() {
            self.env = Some(env);
        }
        self
    }
    /// Child calls chroot after forking from the parent process. To
    /// use this, the child must have CAP_SYS_CHROOT.
    pub fn chroot(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.chroot = Some(path.into());
        self
    }
    /// Child changes directory after forking from the parent process.
    /// If a chroot has also been specified, the directory change will
    /// happen after the chroot.
    pub fn chdir(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.chdir = Some(path.into());
        self
    }
    /// Adds the fd to a list of file descriptors to close after the
    /// main process forks, just before the exec call for the child
    /// process. Any fds that you don't want the child process to
    /// inherit, and doesn't have the CLOEXEC flag set, may be handled
    /// by calling this method. Note that the fds in the main process
    /// are not affected.
    pub fn fd_close_after_fork(&mut self, fd: impl AsRawFd) -> &mut Self {
        self.fds_to_close.push(fd.as_raw_fd());
        self
    }
    /// Creates a pipe. The read end gets connected to the child
    /// process stdin, and the write end is returned as an OwnedFd.
    pub fn pipe_to_stdin(&mut self) -> OwnedFd {
        let (fd, io_config) = ioconfig::PipeToChild::new(0);
        self.io_configs.push(io_config);
        fd
    }
    /// Creates a pipe. The write end gets connected to the child
    /// process stdout, and the read end is returned as an OwnedFd.
    pub fn pipe_from_stdout(&mut self) -> OwnedFd {
        let (fd, io_config) = ioconfig::PipeFromChild::new(1);
        self.io_configs.push(io_config);
        fd
    }
    /// Creates a pipe. The write end gets connected to the child
    /// process stderr, and the read end is returned as an OwnedFd.
    pub fn pipe_from_stderr(&mut self) -> OwnedFd {
        let (fd, io_config) = ioconfig::PipeFromChild::new(2);
        self.io_configs.push(io_config);
        fd
    }
    /// Redirects stderr to stdout. Equivalent to `2>&1` in a shell
    /// script.
    pub fn redirect21(&mut self) {
        self.config_io(ioconfig::Redirect::new(2, 1));
    }
    fn devnull(&mut self) -> RawFd {
        let f = self.devnull.get_or_insert_with(|| {
            let mut f_opts = File::options();
            f_opts.write(true).read(false).create(false);
            let f = f_opts
                .open(DEVNULL)
                .expect("Should have opened /dev/null for writing");
            f
        });
        f.as_raw_fd()
    }
    /// Redirect specified fd to [`DEVNULL`]
    pub fn quiet_fd(&mut self, fd: RawFd) {
        let raw_fd = self.devnull();
        self.fd_close_after_fork(raw_fd);
        self.config_io(ioconfig::Redirect::new_with_priority(fd, raw_fd, 10));
    }
    /// Redirects stdout and stderr to [`DEVNULL`]
    pub fn quiet(&mut self) {
        self.quiet_fd(1);
        self.quiet_fd(2);
    }
    /// Redirects stdout to [`DEVNULL`]
    pub fn quiet_stdout(&mut self) {
        self.quiet_fd(1);
    }
    /// Redirects stderr to [`DEVNULL`]
    pub fn quiet_stderr(&mut self) {
        self.quiet_fd(2);
    }

    /// Forks and execs a child process, returning a [`Child`] value.
    /// Use [`Child::wait()`] to collect the process exit status.
    pub fn spawn(mut self) -> Child {
        // sort the io configs in priority order
        self.io_configs
            .sort_by(|b, a| a.priority().cmp(&b.priority()));
        let fork_result = unsafe { fork() }.expect("Should have forked");
        if let ForkResult::Parent { child, .. } = fork_result {
            // run parent process post fork hooks
            self.io_configs
                .iter_mut()
                .for_each(|x| x.parent_post_fork());
            return Child::new(child);
        }
        // run child process post fork hooks
        self.io_configs.iter().for_each(|x| x.child_post_fork());
        // close the specified fds
        for fd in self.fds_to_close.iter() {
            _ = nix::unistd::close(*fd);
        }
        if let Some(chroot) = self.chroot.take() {
            nix::unistd::chroot(chroot.as_os_str()).expect("Should have chrooted");
        }
        if let Some(chdir) = self.chdir.take() {
            nix::unistd::chdir(chdir.as_os_str()).expect("Should have changed directory");
        }
        // set up the environment
        if let Some(env) = self.env.take() {
            self.exec_with_env(&env);
        } else {
            self.exec();
        }
        unreachable!()
    }
    pub fn exec(self) {
        _ = nix::unistd::execvp(&self.bin, self.args.as_slice());
    }
    pub fn exec_with_env(self, env: &[CString]) {
        _ = nix::unistd::execvpe(&self.bin, self.args.as_slice(), &env);
    }
}

/// Struct that represents a running or ended child process. Process
/// exit status is collected via `.wait()`.
pub struct Child {
    pid: Arc<Pid>,
}
impl Child {
    fn new(pid: Pid) -> Self {
        Self { pid: Arc::new(pid) }
    }
    /// Returns the pid of the child process.
    pub fn pid(&self) -> Pid {
        self.pid.as_ref().clone()
    }
    /// Blocks until the child process has ended, and returns its exit
    /// status.
    pub fn wait(self) -> Result<WaitStatus, Errno> {
        let pid = self.pid.as_ref().clone();
        let exitstatus = {
            let options = None;
            nix::sys::wait::waitpid(pid, options)
        };
        exitstatus
    }
    /// Returns a [`ChildHandle`] that may be use to send signals to
    /// the the child process from another thread.
    pub fn get_handle(&self) -> ChildHandle {
        let pid_ref = Arc::downgrade(&self.pid);
        ChildHandle::new(pid_ref)
    }
}

#[derive(Debug)]
pub enum SignalError {
    /// Error occurred sending the signal
    Errno(Errno),
    /// The [`Child`] has been dropped, presumably by [`Child::wait()`].
    ChildDropped,
}
impl std::error::Error for SignalError {}
impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<Errno> for SignalError {
    fn from(value: Errno) -> Self {
        Self::Errno(value)
    }
}

/// Holds a reference to a running child process that may be used to
/// send signals to it. Obtained by calling [`Child::get_handle()`].
/// Useful for sending signals to a child that is being waited on by
/// another thread.
///
/// Example:
/// ```
/// # use creche::*;
/// # use std::thread::{sleep, spawn};
/// # use std::time::Duration;
/// // sleep for a few seconds
/// # use creche::*;
/// let mut cmd = ChildBuilder::new("sleep");
/// cmd.arg("6");
/// println!("sleeping for six seconds");
/// let child = cmd.spawn();
///
/// // get a "handle" to the child process so that we may signal it
/// let handle = child.get_handle();
///
/// // spawn a thread that will send SIGTERM, terminating it early
/// spawn( move || {
///     sleep(Duration::from_secs(2));
///     println!("sending SIGTERM from thread");
///     _ = handle.terminate();
/// });
///
/// // collect the child exit status
/// println!("child exit: {:?}", child.wait());
/// ```
pub struct ChildHandle {
    inner: Weak<Pid>,
}
impl ChildHandle {
    fn new(inner: Weak<Pid>) -> Self {
        Self { inner }
    }
    /// Sends the signal to the child process. Returns the raw error
    /// number if the signal was not sent. If this method is called
    /// after the originating [`Child`] is dropped,
    /// `Err(SignalError::ChildDropped)` will be returned.
    pub fn kill(&self, signal: Signal) -> Result<(), SignalError> {
        if let Some(pid) = Weak::upgrade(&self.inner) {
            nix::sys::signal::kill(*pid, signal)?;
            Ok(())
        } else {
            Err(SignalError::ChildDropped)
        }
    }
    /// Convenience method for sending SIGTERM that drops `self`.
    pub fn terminate(&self) -> Result<(), SignalError> {
        self.kill(Signal::SIGTERM)
    }
    /// Sends SIGHUP to the child process.
    pub fn hup(&self) -> Result<(), SignalError> {
        self.kill(Signal::SIGHUP)
    }
    /// Sends SIGUSR1 to the child process.
    pub fn sigusr1(&self) -> Result<(), SignalError> {
        self.kill(Signal::SIGUSR1)
    }
    /// Sends SIGUSR2 to the child process.
    pub fn sigusr2(&self) -> Result<(), SignalError> {
        self.kill(Signal::SIGUSR2)
    }
}
