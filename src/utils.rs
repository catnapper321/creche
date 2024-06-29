#[derive(Debug)]
pub enum HeadBodyTail<T> {
    Only(T),
    Head(T),
    Body(T),
    Tail(T),
}

#[derive(Default)]
pub struct HeadBodyTailIter<T, I: Iterator<Item = T>> {
    next_item: Option<T>,
    first: bool,
    inner: I,
}

impl<T, I: Iterator<Item = T>> HeadBodyTailIter<T, I> {
    pub fn new(mut inner: I) -> Self {
        let next_item = inner.next();
        Self {
            inner,
            first: true,
            next_item,
        }
    }
}

impl<T, I: Iterator<Item = T>> Iterator for HeadBodyTailIter<T, I> {
    type Item = HeadBodyTail<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let current_item = self.next_item.take();
        if current_item.is_none() {
            return None;
        }
        self.next_item = self.inner.next();
        let result = match (self.next_item.is_some(), self.first) {
            (true, true) => current_item.map(HeadBodyTail::Head),
            (false, true) => current_item.map(HeadBodyTail::Only),
            (true, false) => current_item.map(HeadBodyTail::Body),
            (false, false) => current_item.map(HeadBodyTail::Tail),
        };
        self.first = false;
        result
    }
}

use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::ffi::{CStr, CString};

/// Helper type for dealing with different ways to represent child process
/// arguments. Internally, [`ChildBuilder`] operates on CStrings.
pub struct Argument(pub CString);

impl Argument {
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, std::ffi::NulError> {
        let cstring = CString::new(value.into())?;
        Ok(Self(cstring))
    }
    pub fn into_value(self) -> CString {
        self.0
    }
}

impl std::ops::Deref for Argument {
    type Target = CString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Argument {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<&std::path::Path> for Argument {
    fn from(value: &std::path::Path) -> Self {
        let x: Vec<u8> = value.as_os_str().as_bytes().into();
        Self(CString::new(x).unwrap())
    }
}

impl From<std::path::PathBuf> for Argument {
    fn from(value: std::path::PathBuf) -> Self {
        value.as_path().into()
    }
}

impl From<&std::ffi::OsStr> for Argument {
    fn from(value: &std::ffi::OsStr) -> Self {
        let x: Vec<u8> = value.as_bytes().into();
        Self(CString::new(x).unwrap())
    }
}

impl From<std::ffi::OsString> for Argument {
    fn from(value: std::ffi::OsString) -> Self {
        value.as_os_str().into()
    }
}

impl From<&str> for Argument {
    fn from(value: &str) -> Self {
        let x: Vec<u8> = value.as_bytes().into();
        Self(CString::new(x).unwrap())
    }
}

impl From<String> for Argument {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<&CStr> for Argument {
    fn from(value: &CStr) -> Self {
        Self(value.to_owned())
    }
}

impl From<CString> for Argument {
    fn from(value: CString) -> Self {
        Self(value)
    }
}
