use crate::application::NavigationFailure;
use std::fmt;

/// Runtime failures are structured so callers can make decisions without parsing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A runtime-wide configuration value was invalid.
    InvalidConfiguration {
        /// Name of the rejected configuration field.
        field: &'static str,
        /// Validation failure explanation.
        reason: String,
    },
    /// Application package metadata did not pass validation.
    InvalidManifest {
        /// Name of the rejected application manifest field.
        field: &'static str,
        /// Validation failure explanation.
        reason: String,
    },
    /// An application with the same package identifier is already registered.
    ApplicationAlreadyLoaded {
        /// Conflicting application identifier.
        app_id: String,
    },
    /// No application with the requested package identifier is registered.
    ApplicationNotFound {
        /// Requested application identifier.
        app_id: String,
    },
    /// A lifecycle operation is invalid for the application's current state.
    InvalidState {
        /// Application identifier for the rejected transition.
        app_id: String,
        /// State observed when the operation was requested.
        state: String,
        /// Lifecycle operation requested.
        operation: &'static str,
    },
    /// The application context has already been destroyed.
    ApplicationDestroyed {
        /// Destroyed application identifier.
        app_id: String,
    },
    /// The application's private storage namespace has been released.
    ApplicationStorageClosed {
        /// Application identifier owning the released storage namespace.
        app_id: String,
    },
    /// The application's virtual filesystem namespace has been released.
    ApplicationFileSystemClosed {
        /// Application identifier that owns the released filesystem namespace.
        app_id: String,
    },
    /// A virtual filesystem path is absolute, non-normalized or contains an invalid component.
    InvalidFileSystemPath {
        /// Application identifier that owns the path namespace.
        app_id: String,
        /// Path supplied by the caller.
        path: String,
    },
    /// A virtual filesystem resource ceiling would be exceeded.
    FileSystemQuotaExceeded {
        /// Application identifier that owns the filesystem namespace.
        app_id: String,
        /// Resource ceiling name.
        quota: &'static str,
        /// Configured limit.
        limit: usize,
        /// Requested or resulting amount.
        attempted: usize,
    },
    /// Page navigation could not be applied to the application's registered routes or page stack.
    NavigationFailed {
        /// Application identifier owning the rejected navigation.
        app_id: String,
        /// Typed explanation for the rejected navigation.
        reason: NavigationFailure,
    },
    /// JavaScript context initialization or execution failed.
    JavaScriptExecution {
        /// Application identifier that owns the failed context.
        app_id: String,
        /// Typed engine failure rendered for diagnostics.
        reason: sdkwork_miniprogram_v8_adapter::V8Error,
    },
    /// A runtime synchronization primitive was poisoned.
    LockPoisoned {
        /// Runtime resource guarded by the poisoned synchronization primitive.
        resource: &'static str,
    },
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
            Self::ApplicationFileSystemClosed { app_id } => {
                write!(formatter, "filesystem for application `{app_id}` is closed")
            }
            Self::InvalidFileSystemPath { app_id, path } => {
                write!(
                    formatter,
                    "invalid virtual filesystem path `{path}` for `{app_id}`"
                )
            }
            Self::FileSystemQuotaExceeded {
                app_id,
                quota,
                limit,
                attempted,
            } => write!(
                formatter,
                "virtual filesystem quota `{quota}` exceeded for `{app_id}`: {attempted} > {limit}"
            ),
            Self::NavigationFailed { app_id, reason } => {
                write!(
                    formatter,
                    "navigation failed for application `{app_id}`: {reason:?}"
                )
            }
            Self::JavaScriptExecution { app_id, reason } => {
                write!(
                    formatter,
                    "JavaScript execution failed for `{app_id}`: {reason}"
                )
            }
            Self::LockPoisoned { resource } => {
                write!(formatter, "runtime lock poisoned for {resource}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}
