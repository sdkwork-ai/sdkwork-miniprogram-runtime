//! Mini Program execution-engine configuration, application lifecycle, and isolation primitives.
//!
//! This crate deliberately contains no platform-specific execution or rendering
//! implementation. Those capabilities are supplied by adapters in other crates.

mod application;
mod config;
mod error;
mod runtime;
mod storage;

pub use application::{ApplicationHandle, ApplicationManifest, ApplicationState};
pub use config::{EngineKind, LogLevel, RendererKind, ResourceBudget, RuntimeConfig};
pub use error::{Result, RuntimeError};
pub use runtime::{Runtime, RuntimeBuilder};
pub use storage::ApplicationStorage;
