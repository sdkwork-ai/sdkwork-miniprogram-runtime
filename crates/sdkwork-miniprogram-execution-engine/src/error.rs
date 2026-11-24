use std::fmt;

/// Runtime failures are structured so callers can make decisions without parsing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A runtime-wide configuration value was invalid.
    InvalidConfiguration { field: &'static str, reason: String },
    /// Application package metadata did not pass validation.
    InvalidManifest { field: &'static str, reason: String },
    /// An application with the same package identifier is already registered.
    ApplicationAlreadyLoaded { app_id: String },
    /// No application with the requested package identifier is registered.
    ApplicationNotFound { app_id: String },
    /// A lifecycle operation is invalid for the application's current state.
    InvalidState {
        app_id: String,
        state: String,
        operation: &'static str,
    },
    /// The application context has already been destroyed.
    ApplicationDestroyed { app_id: String },
    /// The application's private storage namespace has been released.
    ApplicationStorageClosed { app_id: String },
    /// A runtime synchronization primitive was poisoned.
    LockPoisoned { resource: &'static str },
}

/// Result type used by the runtime execution engine.
pub type Result<T> = std::result::Result<T, RuntimeError>;

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration { field, reason } => {
                write!(
                    formatter,
                    "invalid runtime configuration field `{field}`: {reason}"
                )
            }
            Self::InvalidManifest { field, reason } => {
                write!(
                    formatter,
                    "invalid application manifest field `{field}`: {reason}"
                )
            }
            Self::ApplicationAlreadyLoaded { app_id } => {
                write!(formatter, "application `{app_id}` is already loaded")
            }
            Self::ApplicationNotFound { app_id } => {
                write!(formatter, "application `{app_id}` is not loaded")
            }
            Self::InvalidState {
                app_id,
                state,
                operation,
            } => {
                write!(
                    formatter,
                    "cannot {operation} application `{app_id}` while in state `{state}`"
                )
            }
            Self::ApplicationDestroyed { app_id } => {
                write!(formatter, "application `{app_id}` has been destroyed")
            }
            Self::ApplicationStorageClosed { app_id } => {
                write!(formatter, "storage for application `{app_id}` is closed")
            }
            Self::LockPoisoned { resource } => {
                write!(formatter, "runtime lock poisoned for {resource}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}
