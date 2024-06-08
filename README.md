# Creche

Configure, run, and monitor single child processes or entire pipelines
of processes. Redirect file descriptors to and from those processes.
Control environment variables.

Creche is an alternative to `Command` and friends in the standard library.

## Goals

- Minimal dependencies
- No macros, just regular Rust syntax
- Keep the easy stuff easy, without sacrificing configurability

## Limitations

- Linux only
- Doesn't support `nostd`
- Doesn't handle child process priveleges/capabilities yet
- Doesn't do chdir/chroot yet
- No async support yet

## Examples

Read output of `ls` into a `String`:

```rust
// configure the child process
let mut cmd = ChildBuilder::new("cat");
cmd.arg("somefile")
    .redirect21(); // redirect stderr to stdout
let read_fd = cmd.pipe_from_stdout();

// run it
let mut child = cmd.spawn();

// read stdout from the child process
let f = std::fs::File::from(read_fd);
let output = std::io::read_to_string(f)?;
println!("output is: {:?}", output);
println!("exit status: {:?}", child.wait());
```

Write data to a child process:

```rust
let mut cmd = creche::ChildBuilder::new("tr");
cmd.arg("[:lower:]")
    .arg("[:upper:]");
let write_fd = cmd.pipe_to_stdin();
let child = cmd.spawn();

// write some data
let mut f = std::fs::File::from(write_fd);
writeln!(f, "this is a test message");
// don't forget to close your fd. Or at least .flush() the thing.
drop(f);

println!("child exit: {:?}", child.wait());
```

Set up [fzf](https://github.com/junegunn/fzf) to run in an environment
with just PATH, HOME, and FZF_DEFAULT_COMMAND. Then run it and get its
output:

```rust
// configure the environment
let mut env = envconfig::EnvironmentBuilder::new();
env.keep("PATH")
    .set("HOME", "/etc")
    // Note: $HOME is expanded in the shell spawned by fzf.
    // It's not creche magic.
    .set("FZF_DEFAULT_COMMAND", "ls -1 $HOME");
// configure the child process
let mut cmd = ChildBuilder::new("fzf");
cmd.arg("-m")
    .arg(r#"--preview=bat $HOME/{}"#)
    .set_env(env.realize());
let read_fd = cmd.pipe_from_stdout();

// run it
let child = cmd.spawn();

// read the result
let mut f = std::fs::File::from(read_fd);
let output = std::io::read_to_string(f)?;
println!("output is: {:?}", output);
println!("exit status: {:?}", child.wait());
```

Pipe the output of `ls` into `sort` into `tr`. Redirect stderr for all of that to /dev/null:

```rust
let mut ls_cmd = ChildBuilder::new("ls");
ls_cmd.arg("-1");
let mut sort_cmd = ChildBuilder::new("sort");
sort_cmd.arg("-r");
let mut tr_cmd = ChildBuilder::new("tr");
tr_cmd.arg("[:lower:]");
tr_cmd.arg("[:upper:]");

let mut pipeline = SimplePipelineBuilder::new();
let mut children = pipeline
    .add_builder(ls_cmd)
    .add_builder(sort_cmd)
    .add_builder(tr_cmd)
    .quiet()
    .spawn();

println!("{:?}", children.wait());
```
