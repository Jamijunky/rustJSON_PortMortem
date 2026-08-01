//! cjson-rs: a faithful Rust port of [cJSON](https://github.com/DaveGamble/cJSON).
//!
//! This crate is organized in two layers:
//! - a raw, ABI-compatible core ([`model`], [`alloc`], [`parse`], [`print`],
//!   [`float`], [`manip`]) that mirrors the C implementation byte for byte,
//! - an `extern "C"` surface ([`ffi`]) exposing every cJSON symbol so the
//!   original C test suite can link against the port unmodified.

#![allow(clippy::missing_safety_doc)]

pub mod alloc;
pub mod float;
pub mod manip;
pub mod model;
pub mod parse;
pub mod print;
pub mod utils;
pub mod ffi;
