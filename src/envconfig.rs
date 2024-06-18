use crate::*;
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
