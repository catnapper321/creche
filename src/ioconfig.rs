use super::*;
use nix::{
    fcntl::OFlag,
    unistd::{dup2, pipe2},
};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

pub type IOConfig = Box<dyn ConfigureIO>;

// returns (read_end_fd, write_end_fd) of a pipe with CLOEXEC flag set
fn cloexec_pipe() -> (OwnedFd, OwnedFd) {
    let mut flags = OFlag::empty();
    flags.set(OFlag::O_CLOEXEC, true);
    pipe2(flags).expect("Should have created a new pipe")
}

pub trait ConfigureIO {
    // NOTE: do not drop OwnedFds in the child hook, as this confuses
    // the borrow checker. Close RawFds instead. The forked process
    // execs anyway, so ownership doesn't matter here.
    // child must close all fds after dup2 that are not being used
    // by that process.
    // We rely on setting the CLOEXEC flag on fds to manage this
    // NOTE: fd redirects should happen after pipelines are configured
    fn child_post_fork(&self) {}
    // NOTE: drop OwnedFds in the parent hook. parent should drop all
    // fds that are not writing to/reading from a child process
    fn parent_post_fork(&mut self) {}
    // returns the setup priority for the config. Lower value gets
    // configured after higher values. Necessary so that redirects
    // may override pipeline configurations.
    fn priority(&self) -> i32 {
        100
    }
}

/// Generic fd redirect. Simply `dup2()`s the from_fd into the to_fd.
/// Does not close any file descriptors in the parent or child
/// processes.
pub struct Redirect {
    from_fd: RawFd,
    to_fd: RawFd,
    priority: i32,
}
impl ConfigureIO for Redirect {
    fn child_post_fork(&self) {
        _ = dup2(self.to_fd, self.from_fd);
    }
    fn parent_post_fork(&mut self) {}
    fn priority(&self) -> i32 {
        self.priority
    }
}
impl Redirect {
    /// Redirects from_fd to to_fd. For example, to redirect stderr to
    /// stdout: `Redirect::new(2, 1)`. Or just use the
    /// [`ChildBuilder::redirect21()`] convenience method.
    pub fn new(from_fd: RawFd, to_fd: RawFd) -> IOConfig {
        Box::new(Self {
            from_fd,
            to_fd,
            priority: 0,
        })
    }
    /// Allows setting a priority for the redirect. Lower numbers are
    /// configured after higher ones (i.e. lower numbers override
    /// higher)
    pub fn new_with_priority(from_fd: RawFd, to_fd: RawFd, priority: i32) -> IOConfig {
        Box::new(Self {
            from_fd,
            to_fd,
            priority,
        })
    }
}

/// Creates a pipe. Connects the write end of the pipe to the specified
/// fd of the child process, and returns an OwnedFd for the read end.
///
/// For example, to read data from stdout of a child process, use
/// `PipeFromChild::new(1)`. Or just use the
/// [`ChildBuilder::pipe_from_stdout()`] convenience method.
pub struct PipeFromChild {
    child_fd: RawFd,
    pipe_w: Option<OwnedFd>,
}
impl ConfigureIO for PipeFromChild {
    fn child_post_fork(&self) {
        let write_fd = self
            .pipe_w
            .as_ref()
            .map(|x| x.as_raw_fd())
            .expect("Should have an OwnedFd for write end of pipe");
        // redirect the child_fd to the write end of the pipe
        _ = dup2(write_fd, self.child_fd);
    }

    fn parent_post_fork(&mut self) {
        // close the write side of the pipe
        self.pipe_w.take();
    }
}
impl PipeFromChild {
    /// Creates a pipe. Connects the write end of the pipe to the
    /// specified fd of the child process, and returns an OwnedFd for
    /// the read end. The returned IOConfig should be used with
    /// [`ChildBuilder::config_io()`]
    ///
    /// Panics if a new pipe cannot be created.
    pub fn new(child_fd: RawFd) -> (OwnedFd, IOConfig) {
        let (pipe_r, pipe_w) = cloexec_pipe();
        (
            pipe_r,
            Box::new(Self {
                child_fd,
                pipe_w: Some(pipe_w),
            }),
        )
    }
}

/// Creates a pipe. Connects the read end of the pipe to the specified
/// fd of the child process, and returns an OwnedFd for the write end.
///
/// For example, to write data to stdin of a child process, use
/// `PipeToChild::new(0)`. Or just use the
/// [`ChildBuilder::pipe_to_stdin()`] convenience method.
pub struct PipeToChild {
    child_fd: RawFd,
    pipe_r: Option<OwnedFd>,
}
impl ConfigureIO for PipeToChild {
    fn child_post_fork(&self) {
        let read_fd = self
            .pipe_r
            .as_ref()
            .map(|x| x.as_raw_fd())
            .expect("Should have an OwnedFd for read end of pipe");
        // redirect the child_fd to the read end of the pipe
        _ = dup2(read_fd, self.child_fd);
    }

    fn parent_post_fork(&mut self) {
        // close the read side of the pipe
        self.pipe_r.take();
    }
}
impl PipeToChild {
    /// Creates a pipe. Connects the write end of the pipe to the
    /// specified fd of the child process, and returns an OwnedFd for
    /// the read end. The returned IOConfig should be used with
    /// [`ChildBuilder::config_io()`]
    ///
    /// Panics if a new pipe cannot be created.
    pub fn new(child_fd: RawFd) -> (OwnedFd, IOConfig) {
        let (pipe_r, pipe_w) = cloexec_pipe();
        (
            pipe_w,
            Box::new(Self {
                child_fd,
                pipe_r: Some(pipe_r),
            }),
        )
    }
}

/// Creates a pipe and returns each end as an [`IOConfig`] trait
/// object. These can be used to manually set up pipelines between
/// child processes.
///
/// Example:
/// ```
/// # use creche::*;
/// # use std::io::Write;
/// // create a pipe that will connect stdout from a child process to stdin
/// // of another child process.
/// let (mut r, mut w) = ioconfig::interprocess_pipe(0, 1);
///
/// // build the child processes, piping output of tr into sort
/// let mut tr_cmd = ChildBuilder::new("tr");
/// tr_cmd.arg("[:lower:]").arg("[:upper:]");
/// tr_cmd.config_io(w);
/// let mut sort_cmd = ChildBuilder::new("sort");
/// sort_cmd.config_io(r);
///
/// // pipe to stdin of the tr process
/// let pipe = tr_cmd.pipe_to_stdin();
/// // start the child processes
/// let sort_child = sort_cmd.spawn();
/// let tr_child = tr_cmd.spawn();
///
/// // write some data. Stdout of sort is inherited from the main process.
/// let data = "this is a test message".split_whitespace();
/// let mut f = std::fs::File::from(pipe);
/// for line in data {
///     writeln!(f, "{}", line);
/// }
/// drop(f);
///
/// println!("sort cmd exit: {:?}", sort_child.wait());
/// println!("tr cmd exit  : {:?}", tr_child.wait());
/// ```
pub fn interprocess_pipe(
    read_fd: RawFd,
    write_fd: RawFd,
) -> (Box<InterprocessPipeRead>, Box<InterprocessPipeWrite>) {
    let (r, w) = cloexec_pipe();
    (
        Box::new(InterprocessPipeRead {
            child_fd: read_fd,
            read_fd: Some(r),
        }),
        Box::new(InterprocessPipeWrite {
            child_fd: write_fd,
            write_fd: Some(w),
        }),
    )
}

pub struct InterprocessPipeWrite {
    write_fd: Option<OwnedFd>,
    child_fd: RawFd,
}
impl ConfigureIO for InterprocessPipeWrite {
    fn child_post_fork(&self) {
        let fd = self.write_fd.as_ref().map(|x| x.as_raw_fd()).unwrap();
        // stdout usually
        _ = dup2(fd, self.child_fd);
    }

    fn parent_post_fork(&mut self) {
        self.write_fd.take();
    }
}

pub struct InterprocessPipeRead {
    read_fd: Option<OwnedFd>,
    child_fd: RawFd,
}
impl ConfigureIO for InterprocessPipeRead {
    fn child_post_fork(&self) {
        let fd = self.read_fd.as_ref().map(|x| x.as_raw_fd()).unwrap();
        // stdin usually
        _ = dup2(fd, self.child_fd);
    }

    fn parent_post_fork(&mut self) {
        self.read_fd.take();
    }
}

/// Takes anything that implements `Into<OwnedFd>` and redirects
/// the specified child process fd to it.
///
/// Example redirect stderr to a log file:
/// ```
/// # use creche::*;
/// # fn example() -> std::io::Result<()> {
/// // set up log file
/// let mut f_opts = std::fs::File::options();
/// f_opts.write(true).read(false).create(true);
/// let file = f_opts.open("logfile")?;
///
/// // build the child process
/// let mut cmd = ChildBuilder::new("cat");
/// cmd.arg("somefile");
/// let logfile_io_config = ioconfig::RedirectToFd::new(file, 2);
/// cmd.config_io(logfile_io_config);
///
/// let mut child = cmd.spawn();
/// println!("{:?}", child.wait());
/// # Ok(()) }
/// ```
///
/// Example redirect from a file to stdin of a child process:
/// ```
/// # use creche::*;
/// # fn example() -> std::io::Result<()> {
/// // set up the file to read from
/// let mut f_opts = std::fs::File::options();
/// f_opts.read(true).write(false);
/// let file = f_opts.open("logfile")?;
///
/// let mut cmd = ChildBuilder::new("cat");
/// let file_io_config = ioconfig::RedirectToFd::new(file, 0);
/// cmd.config_io(file_io_config);
///
/// let mut child = cmd.spawn();
/// println!("{:?}", child.wait());
/// # Ok(()) }
/// ```

pub struct RedirectToFd {
    owned_fd: Option<OwnedFd>,
    child_fd: RawFd,
}
impl ConfigureIO for RedirectToFd {
    fn child_post_fork(&self) {
        let fd = self.owned_fd.as_ref().map(|x| x.as_raw_fd()).unwrap();
        _ = dup2(fd, self.child_fd);
        _ = nix::unistd::close(fd).expect("Should have closed fd");
    }

    fn parent_post_fork(&mut self) {
        self.owned_fd.take();
    }
}
impl RedirectToFd {
    /// Redirects the child process fd to the provided OwnedFd.
    pub fn new(owned_fd: impl Into<OwnedFd>, child_fd: RawFd) -> IOConfig {
        Box::new(Self {
            owned_fd: Some(owned_fd.into()),
            child_fd,
        })
    }
}
