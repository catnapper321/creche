//! Configure, run, and monitor single child processes or entire pipelines of processes.
//! [`ChildBuilder`] is the heart of this crate, it is responsible for
//! starting child processes. The
//! [`ioconfig`] module contains data structures that are used by
//! `ChildBuilder` to create pipes and do various operations with file
//! descriptors. The [`envconfig`] module has a builder for process
//! environments - use it to set environment variables and control what a
//! child process inherits from the parent process environment. The types
//! in the [`pipeline`] module are conveniences for constructing and
//! monitoring common case pipelines.

#![allow(unused_imports)]
pub const DEVNULL: &str = "/dev/null";

// re-exports
pub use creche::{Child, ChildBuilder};
pub use pipeline::{PipelineChildren, SimplePipelineBuilder};

/// Types used to configure child process file descriptors. The values
/// obtained via the functions in this module are used with
/// [`ChildBuilder::config_io()`].
pub mod ioconfig {
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
    /// ```
    ///
    /// Example redirect from a file to stdin of a child process:
    /// ```
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
}

/// Configure child processes
mod creche {
    use super::*;
    use nix::unistd::{execvp, fork, ForkResult, Pid};
    pub use nix::{errno::Errno, sys::wait::WaitStatus};
    use std::{
        ffi::{CString, OsString},
        fs::File,
        os::fd::{AsRawFd, OwnedFd, RawFd},
    };

    /// Struct for configuration of a child process. File descriptors are
    /// configured by calling [`Self::config_io()`] with values obtained
    /// from the [`ioconfig`] module. Environment is configured via
    /// [`Self::set_env()`] with a value obtained from
    /// [`envconfig::EnvironmentBuilder`].
    ///
    /// Example of piping data to a child process:
    /// ```
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
    }
    impl ChildBuilder {
        pub fn new(path: impl Into<Vec<u8>>) -> Self {
            let p = CString::new(path.into()).unwrap();
            Self {
                bin: p.clone(),
                args: vec![p], // first arg is always the executable name
                io_configs: Vec::new(),
                devnull: None,
                fds_to_close: Vec::new(),
                env: None,
            }
        }
        pub fn arg(&mut self, arg: impl Into<Vec<u8>>) -> &mut Self {
            let x = CString::new(arg).unwrap();
            self.args.push(x);
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

        /// Forks and execs a child process, returning a [`Child`] value. Use [`Child::wait()`] to collect the process exit status.
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
    /// exit status is available via `.wait()`.
    pub struct Child {
        pid: Option<Pid>,
    }
    impl Child {
        fn new(pid: Pid) -> Self {
            Self { pid: Some(pid) }
        }
        /// Returns the pid of the child process.
        pub fn pid(&self) -> Pid {
            self.pid.unwrap()
        }
        /// Blocks until the child process has ended, and returns its exit
        /// status.
        pub fn wait(mut self) -> Result<WaitStatus, Errno> {
            let pid = self.pid.take().unwrap();
            let exitstatus = {
                let options = None;
                nix::sys::wait::waitpid(pid, options)
            };
            exitstatus
        }
    }
}

/// Helpers for composing child processes into a pipeline. Constructing
/// pipelines manually is tedious and error prone.
/// [`SimplePipelineBuilder`] will handle piping stdout -> stdin between
/// processes and spawn everything in a simpler way than manually
/// configuring pipes. See [`ioconfig::interprocess_pipe()`].
pub mod pipeline {
    use super::*;
    use ioconfig::{interprocess_pipe, InterprocessPipeRead};
    use nix::sys::wait::WaitStatus;
    use std::{
        fs::File,
        os::fd::{AsRawFd, RawFd},
    };
    use utils::*;

    /// Constructs a pipeline that connects stdout of a child process to
    /// stdin of the next child process, except for stdin of the first
    /// process and stdout of the final process.
    ///
    /// The [`Self::quiet()`] method may be used to suppress stderr output.
    ///
    /// The general pattern for constructing pipelines is to build each
    /// child process first, configuring their environments and fd
    /// redirects to files and such first - everything except the
    /// interprocess pipe between stdout and stdin. Then add the
    /// `ChildBuilder`s to the pipeline.
    ///
    /// Example:
    /// ```
    ///    let mut ls_cmd = ChildBuilder::new("ls");
    ///    ls_cmd.arg("-1");
    ///    let mut sort_cmd = ChildBuilder::new("sort");
    ///    sort_cmd.arg("-r");
    ///    let mut tr_cmd = ChildBuilder::new("tr");
    ///    tr_cmd.arg("[:lower:]");
    ///    tr_cmd.arg("[:upper:]");
    ///
    ///    let mut pipeline = SimplePipelineBuilder::new();
    ///    let mut children = pipeline
    ///        .add_builder(ls_cmd)
    ///        .add_builder(sort_cmd)
    ///        .add_builder(tr_cmd)
    ///        .quiet()
    ///        .spawn();
    ///
    ///    println!("{:?}", children.wait());
    /// ```
    #[derive(Default)]
    pub struct SimplePipelineBuilder {
        builders: Vec<ChildBuilder>,
        devnull: Option<File>,
        env: Option<envconfig::Environment>,
    }
    impl SimplePipelineBuilder {
        pub fn new() -> Self {
            Self::default()
        }
        /// Adds a [``ChildBuilder``] to the pipeline.
        pub fn add_builder(&mut self, builder: ChildBuilder) -> &mut Self {
            self.builders.push(builder);
            self
        }
        /// Sets an environment for all child processes in the pipeline.
        /// See [`envconfig::EnvironmentBuilder`].
        pub fn set_env(&mut self, env: envconfig::Environment) -> &mut Self {
            self.env = Some(env);
            self
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
        /// Redirects stderr of all child processes to /dev/null. Affects
        /// only the builders added to the pipeline prior to calling this
        /// method. This is a convenience method that replaces configuring
        /// each [`ChildBuilder`] to open /dev/null and redirecting the fd.
        /// To economize file descriptors, it opens /dev/null only once.
        pub fn quiet(&mut self) -> &mut Self {
            let raw_fd = self.devnull();
            for builder in self.builders.iter_mut() {
                builder.config_io(ioconfig::Redirect::new_with_priority(2, raw_fd, 10));
            }
            self
        }
        /// Executes the pipeline. Interprocess pipes are created as needed
        /// to link stdout of a child process to stdin of the next process.
        /// The child processes are started in the reverse of the order in
        /// which they were added to the builder.
        ///
        /// Returns a [`PipelineChildren`] value that may be `.wait()`ed
        /// to get the exit status of each child process.
        pub fn spawn(&mut self) -> PipelineChildren {
            if self.builders.len() == 0 {
                panic!("Pipeline has no processes to spawn");
            }

            // set up interprocess pipes
            let mut pipe_read: Option<Box<InterprocessPipeRead>> = None;
            let builders = HeadBodyTailIterMut::new(&mut self.builders);
            for ref mut builder in builders {
                match builder {
                    HeadBodyTailMut::Only(_) => (),
                    HeadBodyTailMut::Head(x) => {
                        let (r, w) = interprocess_pipe(0, 1);
                        x.config_io(w);
                        pipe_read = Some(r);
                    }
                    HeadBodyTailMut::Body(x) => {
                        let (r, w) = interprocess_pipe(0, 1);
                        x.config_io(w);
                        if let Some(prev_r) = pipe_read.take() {
                            x.config_io(prev_r);
                        }
                        pipe_read = Some(r);
                    }
                    HeadBodyTailMut::Tail(x) => {
                        if let Some(prev_r) = pipe_read.take() {
                            x.config_io(prev_r);
                        }
                    }
                }
            }

            // spawn the processes, last to first
            self.builders.reverse();
            let mut children = Vec::new();
            while let Some(mut builder) = self.builders.pop() {
                // set env
                if let Some(env) = self.env.as_ref() {
                    builder.set_env(env.clone());
                }
                let child = builder.spawn();
                children.push(child);
            }
            PipelineChildren { children }
        }
    }

    /// Struct that contains the ``Child`` values generated by spawning the
    /// ``ChildBuilders`` in the pipeline. Its sole function is to wait on
    /// all of the children processes to terminate, and then return a
    /// ``Vec`` of exit ``WaitStatus``. This status vector is in the same
    /// order that the ``ChildBuilder``s were added to the pipeline
    /// builder.
    pub struct PipelineChildren {
        children: Vec<Child>,
    }
    impl PipelineChildren {
        pub fn wait(&mut self) -> Option<Vec<WaitStatus>> {
            if self.children.len() == 0 {
                return None;
            }
            self.children.reverse();
            let mut wait_results = Vec::new();
            while let Some(child) = self.children.pop() {
                if let Ok(waitstatus) = child.wait() {
                    // child exit
                    wait_results.push(waitstatus);
                } else {
                    // wait error
                    panic!("Child wait error");
                }
            }
            Some(wait_results)
        }
        pub fn children(&self) -> &[Child] {
            self.children.as_ref()
        }
    }
}

pub mod utils {

    #[derive(Debug)]
    pub enum HeadBodyTail<'a, T> {
        Only(&'a T),
        Head(&'a T),
        Body(&'a T),
        Tail(&'a T),
    }
    impl<'a, T> std::ops::Deref for HeadBodyTail<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            match self {
                Self::Only(x) => x,
                Self::Head(x) => x,
                Self::Body(x) => x,
                Self::Tail(x) => x,
            }
        }
    }

    pub struct HeadBodyTailIter<'a, T> {
        index: usize,
        inner: &'a [T],
    }
    impl<'a, T> Iterator for HeadBodyTailIter<'a, T> {
        type Item = HeadBodyTail<'a, T>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index == self.inner.len() {
                return None;
            }
            let i = self.index;
            self.index += 1;
            let element = self.inner.get(i);
            if self.inner.len() == 1 {
                return element.map(Self::Item::Only);
            }
            if i == 0 {
                return element.map(Self::Item::Head);
            }
            if self.index == self.inner.len() {
                return element.map(Self::Item::Tail);
            }
            element.map(Self::Item::Body)
        }
    }
    impl<'a, T> HeadBodyTailIter<'a, T> {
        pub fn new(slice: &'a [T]) -> Self {
            Self {
                index: 0,
                inner: slice,
            }
        }
    }

    #[derive(Debug)]
    pub enum HeadBodyTailMut<'a, T> {
        Only(&'a mut T),
        Head(&'a mut T),
        Body(&'a mut T),
        Tail(&'a mut T),
    }
    impl<'a, T> std::ops::Deref for HeadBodyTailMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            match self {
                Self::Only(x) => x,
                Self::Head(x) => x,
                Self::Body(x) => x,
                Self::Tail(x) => x,
            }
        }
    }
    impl<'a, T> std::ops::DerefMut for HeadBodyTailMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            match self {
                Self::Only(x) => x,
                Self::Head(x) => x,
                Self::Body(x) => x,
                Self::Tail(x) => x,
            }
        }
    }

    pub struct HeadBodyTailIterMut<'a, T> {
        index: usize,
        inner: &'a mut [T],
    }
    impl<'a, T> Iterator for HeadBodyTailIterMut<'a, T> {
        type Item = HeadBodyTailMut<'a, T>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index == self.inner.len() {
                return None;
            }
            let i = self.index;
            self.index += 1;
            // HACK: cast to *mut and back to evade the borrow checker
            // Hopefully the 'a lifetime keeps this safe
            let element = unsafe { self.inner.get_mut(i).map(|z| &mut *(z as *mut T)) };
            if self.inner.len() == 1 {
                return element.map(|x| Self::Item::Only(x));
            }
            if i == 0 {
                return element.map(|x| Self::Item::Head(x));
            }
            if self.index == self.inner.len() {
                return element.map(|x| Self::Item::Tail(x));
            }
            element.map(|x| Self::Item::Body(x))
        }
    }
    impl<'a, T> HeadBodyTailIterMut<'a, T> {
        pub fn new(slice: &'a mut [T]) -> Self {
            Self {
                index: 0,
                inner: slice,
            }
        }
    }
}

/// Generate environments for child processes.
pub mod envconfig {
    use super::*;
    use std::collections::hash_map::{Entry, HashMap, VacantEntry};
    use std::ffi::{CStr, CString, OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    #[derive(Debug)]
    enum Inherit {
        Remove(Vec<OsString>), // vars to remove (blacklist)
        None,                  // clear the environment
        Keep(Vec<OsString>),   // vars to keep (whitelist)
    }
    impl Default for Inherit {
        // Default to inheriting everything
        fn default() -> Self {
            Self::Remove(Vec::new())
        }
    }
    impl Inherit {
        fn clear(&mut self) -> &mut Self {
            *self = Self::None;
            self
        }
        fn remove(&mut self, k: impl Into<OsString>) -> &mut Self {
            match self {
                Self::None | Self::Keep(_) => {
                    *self = Self::Remove(Vec::new());
                    self.remove(k);
                }
                Self::Remove(list) => {
                    list.push(k.into());
                }
            }
            self
        }
        fn keep(&mut self, k: impl Into<OsString>) -> &mut Self {
            match self {
                Self::None | Self::Remove(_) => {
                    *self = Self::Keep(Vec::new());
                    self.keep(k);
                }
                Self::Keep(list) => {
                    list.push(k.into());
                }
            }
            self
        }
        // Returns the full environment
        fn environment(&self) -> EnvironmentHash {
            let mut env = EnvironmentHash::default();
            match self {
                Self::None => (),
                Self::Remove(list) => {
                    for (k, v) in std::env::vars_os() {
                        if !list.contains(&k) {
                            _ = env.insert(k, v);
                        }
                    }
                }
                Self::Keep(list) => {
                    for (k, v) in std::env::vars_os() {
                        if list.contains(&k) {
                            _ = env.insert(k, v);
                        }
                    }
                }
            }
            env
        }
    }

    /// Constructs an environment. The builder sets a policy for inheriting
    /// environment variables from the parent process, and allows setting
    /// new variables and overriding existing values. Use `.realize()` to
    /// get a value that can be used by [`ChildBuilder::set_env()`].
    ///
    /// Example:
    /// ```
    /// // configure the environment
    /// let mut env = envconfig::EnvironmentBuilder::new();
    /// env.keep("HOME").keep("PATH").set("server_port", "1234");
    ///
    /// // configure the child process
    /// let mut cmd = ChildBuilder::new("env");
    /// cmd.set_env(env.realize());
    ///
    /// // run it
    /// let child = cmd.spawn();
    /// println!("{:?}", child.wait());
    /// ```
    #[derive(Default)]
    pub struct EnvironmentBuilder {
        inherit: Inherit,
        set: EnvironmentHash,
    }
    impl EnvironmentBuilder {
        /// Returns a new builder, with the policy to inherit the full
        /// environment of the parent process.
        pub fn new() -> Self {
            let inherit = Inherit::default();
            Self {
                inherit,
                set: EnvironmentHash::default(),
            }
        }
        /// Generates an [`Environment`] that can be used with the
        /// `.set_env()` functions on the process and pipeline builders.
        pub fn realize(self) -> Environment {
            let mut env = self.inherit.environment();
            for (k, v) in self.set.into_iter() {
                _ = env.insert(k, v);
            }
            make_cstring_env(env)
        }
        /// Configures the child to start with a completely empty
        /// environment. Note that you will most likely want to set `PATH`
        /// after calling this method so that the child process executable
        /// can be located. Alternatively, the [`ChildBuilder`] could be
        /// given the full path to the executable.
        pub fn clear(&mut self) -> &mut Self {
            self.inherit.clear();
            self
        }
        /// Sets a variable in the child process environment.
        pub fn set(&mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> &mut Self {
            _ = self.set.insert(k.into(), v.into());
            self
        }
        /// Configures the child environment to inherit only those
        /// variables specified (whitelist policy). Consider keeping `PATH`
        /// when using this method.
        pub fn keep(&mut self, k: impl Into<OsString>) -> &mut Self {
            self.inherit.keep(k);
            self
        }
        /// Configures the child environment to inherit all variables
        /// except for those specified (blacklist policy).
        pub fn remove(&mut self, k: impl Into<OsString>) -> &mut Self {
            self.inherit.remove(k);
            self
        }
    }

    type EnvironmentHash = HashMap<OsString, OsString>;

    /// Data structure that represents a process environment. Used to
    /// configure [`ChildBuilder`] and [`SimplePipelineBuilder`]. Obtained
    /// from [`EnvironmentBuilder::realize()`].
    pub type Environment = Vec<CString>;

    fn make_cstring_env(env: EnvironmentHash) -> Vec<CString> {
        // TODO: return a Result
        use std::io::Write;
        let mut vars = Vec::new();
        // FIX: must ensure no nul bytes and that k does not have =
        for (k, v) in env.iter() {
            let mut buf: Vec<u8> = Vec::new();
            _ = buf.write_all(k.as_bytes());
            _ = buf.write(b"=");
            _ = buf.write_all(v.as_bytes());
            vars.push(CString::new(buf).expect("Should have made CString from a bag of bytes"));
        }
        vars
    }
}
