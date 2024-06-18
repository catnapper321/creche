/// Helpers for composing child processes into a pipeline. Constructing
/// pipelines manually is tedious and error prone.
/// [`SimplePipelineBuilder`] will handle piping stdout -> stdin between
/// processes and spawn everything in a simpler way than manually
/// configuring pipes. See [`ioconfig::interprocess_pipe()`].
use crate::*;
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
        let builders = HeadBodyTailIter::new(self.builders.iter_mut());
        for ref mut builder in builders {
            match builder {
                HeadBodyTail::Only(_) => (),
                HeadBodyTail::Head(x) => {
                    let (r, w) = interprocess_pipe(0, 1);
                    x.config_io(w);
                    pipe_read = Some(r);
                }
                HeadBodyTail::Body(x) => {
                    let (r, w) = interprocess_pipe(0, 1);
                    x.config_io(w);
                    if let Some(prev_r) = pipe_read.take() {
                        x.config_io(prev_r);
                    }
                    pipe_read = Some(r);
                }
                HeadBodyTail::Tail(x) => {
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
