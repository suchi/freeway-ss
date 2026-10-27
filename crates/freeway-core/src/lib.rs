//! Platform-independent scene logic for the Night Freeway SS screensaver.
//!
//! This crate must not depend on any OS API so that it can be unit-tested
//! on every platform (see docs/adr/0001-use-rust-windows-crate-and-direct2d.md).

#![forbid(unsafe_code)]

mod body;
pub mod canvas;
pub mod cruise;
pub mod i18n;
pub mod mountains;
pub mod projection;
pub mod rng;
pub mod road;
pub mod scene;
pub mod scenery;
pub mod settings;
pub mod sky;
pub mod solid;
pub mod traffic;
pub mod tunnel;
pub mod ufo;

pub use canvas::{Canvas, Rgb};
pub use i18n::{DisplayLanguage, Text};
pub use scene::{Scene, SceneConfig, Viewport};
pub use settings::{Language, Settings, SpeedometerStyle};
pub use traffic::{CarColor, CarShapes, LaneChanges, TruckStyle};
pub use tunnel::TunnelLighting;
