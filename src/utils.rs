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

use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::ChildBuilder;
/// Helper type for dealing with different ways to represent child process
/// arguments.
///
/// The `Argument` type converts several other types into the `CStrings`
/// that [`ChildBuilder`] uses internally. Due to [`ChildBuilder::new()`]
/// and [`ChildBuilder::arg()`] taking an `impl Into<Argument>`, these
/// methods will accept `String`s, `PathBuf`s, `OsString`s, and related
/// types and handle the conversion automatically.
#[derive(Debug, Clone)]
pub struct Argument(CString);

impl Argument {
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, std::ffi::NulError> {
        let cstring = CString::new(value.into())?;
        Ok(Self(cstring))
    }
    pub fn into_c_string(self) -> CString {
        self.0
    }
    pub fn into_pathbuf(self) -> PathBuf {
        let bytes = self.0.into_bytes();
        let os_string = unsafe { OsString::from_encoded_bytes_unchecked(bytes) };
        PathBuf::from(os_string)
    }
    pub fn into_os_string(self) -> OsString {
        let bytes = self.0.into_bytes();
        unsafe { OsString::from_encoded_bytes_unchecked(bytes) }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
    pub fn as_os_str(&self) -> &OsStr {
        let bytes = self.as_bytes();
        unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }
    }
    pub fn as_path(&self) -> &Path {
        let x = self.as_os_str();
        Path::new(x)
    }
    /// Convenience method for cloning the inner value as a `CString`.
    pub fn clone_c_string(&self) -> CString {
        self.0.clone()
    }
    /// Convenience method for cloning the inner value as an `OsString`.
    pub fn clone_os_string(&self) -> OsString {
        let bytes = self.0.as_bytes().to_vec();
        unsafe { OsString::from_encoded_bytes_unchecked(bytes) }
    }
    /// Convenience method for cloning the inner value as a `PathBuf`.
    pub fn clone_pathbuf(&self) -> PathBuf {
        let os_string = self.clone_os_string();
        PathBuf::from(os_string)
    }
}

impl std::ops::Deref for Argument {
    type Target = CStr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<&[u8]> for Argument {
    type Error = std::ffi::NulError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::new(value)
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

impl From<&std::path::PathBuf> for Argument {
    fn from(value: &std::path::PathBuf) -> Self {
        From::from(value.clone())
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
        let x: Vec<u8> = value.as_bytes().into();
        Self(CString::new(x).unwrap())
    }
}

impl From<&String> for Argument {
    fn from(value: &String) -> Self {
        From::from(value.clone())
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

impl From<&CString> for Argument {
    fn from(value: &CString) -> Self {
        From::from(value.clone())
    }
}
