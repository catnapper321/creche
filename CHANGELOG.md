# Changelog

## 0.2.0

The `ChildBuilder::new` and `ChildBuilder::arg` methods now take a
`Into<Argument>` type. This breaking change allows the methods to take
`Path`, `OsString`, and similar without explicit conversion:

```rust
let bin_path = PathBuf::from("/usr/bin/ls");
let mut cmd = ChildBuilder::new(bin_path);
```

The Argument type has conversions to and from `&[u8]`, `Vec<u8>`,
`OsString`, `OsStr`, `PathBuf`, and `Path`. An `Argument` may also be created
from `str` and `String` types.
