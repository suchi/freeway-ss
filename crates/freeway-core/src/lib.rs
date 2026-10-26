//! Platform-independent scene logic for the Night Freeway SS screensaver.
//!
//! This crate must not depend on any OS API so that it can be unit-tested
//! on every platform (see docs/adr/0001-use-rust-windows-crate-and-direct2d.md).

#![forbid(unsafe_code)]
