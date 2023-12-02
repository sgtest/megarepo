use super::*;
use crate::sys_common::{AsInner, IntoInner};

use crate::rc::Rc;
use crate::sync::Arc;

#[test]
fn test_os_string_with_capacity() {
    let os_string = OsString::with_capacity(0);
    assert_eq!(0, os_string.inner.into_inner().capacity());

    let os_string = OsString::with_capacity(10);
    assert_eq!(10, os_string.inner.into_inner().capacity());

    let mut os_string = OsString::with_capacity(0);
    os_string.push("abc");
    assert!(os_string.inner.into_inner().capacity() >= 3);
}

#[test]
fn test_os_string_clear() {
    let mut os_string = OsString::from("abc");
    assert_eq!(3, os_string.inner.as_inner().len());

    os_string.clear();
    assert_eq!(&os_string, "");
    assert_eq!(0, os_string.inner.as_inner().len());
}

#[test]
fn test_os_string_capacity() {
    let os_string = OsString::with_capacity(0);
    assert_eq!(0, os_string.capacity());

    let os_string = OsString::with_capacity(10);
    assert_eq!(10, os_string.capacity());

    let mut os_string = OsString::with_capacity(0);
    os_string.push("abc");
    assert!(os_string.capacity() >= 3);
}

#[test]
fn test_os_string_reserve() {
    let mut os_string = OsString::new();
    assert_eq!(os_string.capacity(), 0);

    os_string.reserve(2);
    assert!(os_string.capacity() >= 2);

    for _ in 0..16 {
        os_string.push("a");
    }

    assert!(os_string.capacity() >= 16);
    os_string.reserve(16);
    assert!(os_string.capacity() >= 32);

    os_string.push("a");

    os_string.reserve(16);
    assert!(os_string.capacity() >= 33)
}

#[test]
fn test_os_string_reserve_exact() {
    let mut os_string = OsString::new();
    assert_eq!(os_string.capacity(), 0);

    os_string.reserve_exact(2);
    assert!(os_string.capacity() >= 2);

    for _ in 0..16 {
        os_string.push("a");
    }

    assert!(os_string.capacity() >= 16);
    os_string.reserve_exact(16);
    assert!(os_string.capacity() >= 32);

    os_string.push("a");

    os_string.reserve_exact(16);
    assert!(os_string.capacity() >= 33)
}

#[test]
fn test_os_string_join() {
    let strings = [OsStr::new("hello"), OsStr::new("dear"), OsStr::new("world")];
    assert_eq!("hello", strings[..1].join(OsStr::new(" ")));
    assert_eq!("hello dear world", strings.join(OsStr::new(" ")));
    assert_eq!("hellodearworld", strings.join(OsStr::new("")));
    assert_eq!("hello.\n dear.\n world", strings.join(OsStr::new(".\n ")));

    assert_eq!("dear world", strings[1..].join(&OsString::from(" ")));

    let strings_abc = [OsString::from("a"), OsString::from("b"), OsString::from("c")];
    assert_eq!("a b c", strings_abc.join(OsStr::new(" ")));
}

#[test]
fn test_os_string_default() {
    let os_string: OsString = Default::default();
    assert_eq!("", &os_string);
}

#[test]
fn test_os_str_is_empty() {
    let mut os_string = OsString::new();
    assert!(os_string.is_empty());

    os_string.push("abc");
    assert!(!os_string.is_empty());

    os_string.clear();
    assert!(os_string.is_empty());
}

#[test]
fn test_os_str_len() {
    let mut os_string = OsString::new();
    assert_eq!(0, os_string.len());

    os_string.push("abc");
    assert_eq!(3, os_string.len());

    os_string.clear();
    assert_eq!(0, os_string.len());
}

#[test]
fn test_os_str_default() {
    let os_str: &OsStr = Default::default();
    assert_eq!("", os_str);
}

#[test]
fn into_boxed() {
    let orig = "Hello, world!";
    let os_str = OsStr::new(orig);
    let boxed: Box<OsStr> = Box::from(os_str);
    let os_string = os_str.to_owned().into_boxed_os_str().into_os_string();
    assert_eq!(os_str, &*boxed);
    assert_eq!(&*boxed, &*os_string);
    assert_eq!(&*os_string, os_str);
}

#[test]
fn boxed_default() {
    let boxed = <Box<OsStr>>::default();
    assert!(boxed.is_empty());
}

#[test]
fn test_os_str_clone_into() {
    let mut os_string = OsString::with_capacity(123);
    os_string.push("hello");
    let os_str = OsStr::new("bonjour");
    os_str.clone_into(&mut os_string);
    assert_eq!(os_str, os_string);
    assert!(os_string.capacity() >= 123);
}

#[test]
fn into_rc() {
    let orig = "Hello, world!";
    let os_str = OsStr::new(orig);
    let rc: Rc<OsStr> = Rc::from(os_str);
    let arc: Arc<OsStr> = Arc::from(os_str);

    assert_eq!(&*rc, os_str);
    assert_eq!(&*arc, os_str);

    let rc2: Rc<OsStr> = Rc::from(os_str.to_owned());
    let arc2: Arc<OsStr> = Arc::from(os_str.to_owned());

    assert_eq!(&*rc2, os_str);
    assert_eq!(&*arc2, os_str);
}

#[test]
fn slice_encoded_bytes() {
    let os_str = OsStr::new("123θგ🦀");
    // ASCII
    let digits = os_str.slice_encoded_bytes(..3);
    assert_eq!(digits, "123");
    let three = os_str.slice_encoded_bytes(2..3);
    assert_eq!(three, "3");
    // 2-byte UTF-8
    let theta = os_str.slice_encoded_bytes(3..5);
    assert_eq!(theta, "θ");
    // 3-byte UTF-8
    let gani = os_str.slice_encoded_bytes(5..8);
    assert_eq!(gani, "გ");
    // 4-byte UTF-8
    let crab = os_str.slice_encoded_bytes(8..);
    assert_eq!(crab, "🦀");
}

#[test]
#[should_panic(expected = "byte index 2 is not an OsStr boundary")]
fn slice_mid_char() {
    let crab = OsStr::new("🦀");
    let _ = crab.slice_encoded_bytes(..2);
}

#[cfg(windows)]
#[test]
#[should_panic(expected = "byte index 3 is not an OsStr boundary")]
fn slice_between_surrogates() {
    use crate::os::windows::ffi::OsStringExt;

    let os_string = OsString::from_wide(&[0xD800, 0xD800]);
    assert_eq!(os_string.as_encoded_bytes(), &[0xED, 0xA0, 0x80, 0xED, 0xA0, 0x80]);
    let _ = os_string.slice_encoded_bytes(..3);
}

#[cfg(windows)]
#[test]
fn slice_surrogate_edge() {
    use crate::os::windows::ffi::OsStringExt;

    let os_string = OsString::from_wide(&[0xD800]);
    let mut with_crab = os_string.clone();
    with_crab.push("🦀");

    assert_eq!(with_crab.slice_encoded_bytes(..3), os_string);
    assert_eq!(with_crab.slice_encoded_bytes(3..), "🦀");
}
