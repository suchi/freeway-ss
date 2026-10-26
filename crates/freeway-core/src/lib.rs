//! Platform-independent scene logic for the Night Freeway SS screensaver.
//!
//! This crate must not depend on any OS API so that it can be unit-tested
//! on every platform (see docs/adr/0001-use-rust-windows-crate-and-direct2d.md).

#![forbid(unsafe_code)]

pub mod canvas;
pub mod projection;
pub mod rng;
pub mod road;
pub mod scene;
pub mod settings;
pub mod sky;
pub mod traffic;
pub mod ufo;

pub use canvas::{Canvas, Rgb};
pub use scene::{Scene, SceneConfig, Viewport};
pub use settings::Settings;
