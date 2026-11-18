//! `#[rux::host_api]` / `#[derive(NativeType)]` against a mocked native side.

use std::cell::RefCell;
use std::rc::Rc;

use rux::native::{NativeError, decode, ok_reply, testing};
use rux::prelude::*;

#[derive(NativeType, Debug, Clone, PartialEq)]
pub struct Info {
    pub name: String,
    pub level: Option<i32>,
    pub tags: Vec<String>,
    pub kind: Kind,
}

#[derive(NativeType, Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Phone,
    Tablet,
}

/// Docs are kept.
#[rux::host_api]
pub trait DemoApi {
    fn info(&self) -> Result<Info, NativeError>;
    fn greet(&self, name: String, times: i32) -> Result<String, NativeError>;
    #[rux::async_call]
    fn later(&self, fail: bool) -> Result<(), NativeError>;
}

fn sample() -> Info {
    Info {
        name: "Pixel".into(),
        level: Some(80),
        tags: vec!["a".into()],
        kind: Kind::Tablet,
    }
}

#[test]
fn calls_reach_the_native_side_typed() {
    assert_eq!(DemoApi::API, "rux.DemoApi");
    let _native = testing::mock(|api, method, args| {
        assert_eq!(api, "rux.DemoApi");
        match method {
            0 => Ok(ok_reply(&sample())),
            1 => {
                let mut r = rux::native::Reader::new(args);
                let name = <String as rux::native::Codec>::decode(&mut r).unwrap();
                let times = <i32 as rux::native::Codec>::decode(&mut r).unwrap();
                Ok(ok_reply(&format!("hi {name} x{times}")))
            }
            2 if decode::<bool>(args).unwrap() => Err(NativeError::new("denied", "no")),
            2 => Ok(ok_reply(&())),
            _ => unreachable!(),
        }
    });

    let api = DemoApi::new();
    assert_eq!(api.info(), Ok(sample()));
    assert_eq!(api.greet("Ann".into(), 2).as_deref(), Ok("hi Ann x2"));

    let results = Rc::new(RefCell::new(Vec::new()));
    for fail in [false, true] {
        let results = results.clone();
        api.later(fail, move |r| results.borrow_mut().push(r));
    }
    assert_eq!(
        *results.borrow(),
        [Ok(()), Err(NativeError::new("denied", "no"))]
    );
}

#[test]
fn without_a_native_side_calls_fail_cleanly() {
    assert_eq!(DemoApi::new().info().unwrap_err().code, "no_transport");
}
