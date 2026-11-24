//! Mini Program execution-engine configuration, application lifecycle, and isolation primitives.
//!
//! This crate deliberately contains no platform-specific execution or rendering
//! implementation. Those capabilities are supplied by adapters in other crates.

mod application;
mod config;
mod debug;
mod error;
mod file_system;
mod runtime;
mod storage;

pub use application::{
    ApplicationHandle, ApplicationManifest, ApplicationState, NavigationFailure,
};
pub use config::{EngineKind, LogLevel, RendererKind, ResourceBudget, RuntimeConfig};
pub use debug::{DEFAULT_DEBUG_EVENT_LIMIT, DebugEvent, DebugEventKind, DebugJournal};
pub use error::{Result, RuntimeError};
pub use file_system::{ApplicationVirtualFileSystem, VirtualDirectoryEntry, VirtualFileMetadata};
pub use runtime::{Runtime, RuntimeBuilder};
pub use storage::ApplicationStorage;
