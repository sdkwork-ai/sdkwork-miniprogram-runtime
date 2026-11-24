use crate::{Result, RuntimeError};

/// JavaScript engine requested by a runtime instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// V8 JavaScript engine adapter.
    V8,
}

/// Rendering strategy requested by a runtime instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    /// A platform `WebView` renderer.
    WebView,
    /// A renderer that records a platform-neutral component tree.
    Headless,
}

/// Minimum diagnostic verbosity emitted by runtime components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Error diagnostics only.
    Error,
    /// Warnings and errors.
    Warn,
    /// Informational diagnostics and higher severity messages.
    Info,
    /// Debug diagnostics and higher severity messages.
    Debug,
    /// Trace-level diagnostics.
    Trace,
}

/// Per-application resource ceilings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBudget {
    /// Maximum number of applications loaded into one runtime.
    pub max_applications: usize,
    /// Maximum number of registered pages per application.
    pub max_pages_per_application: usize,
    /// Maximum number of in-memory storage keys per application.
    pub max_storage_entries_per_application: usize,
    /// Maximum encoded size of one storage value in bytes.
    pub max_storage_value_bytes: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_applications: 8,
            max_pages_per_application: 128,
            max_storage_entries_per_application: 1024,
            max_storage_value_bytes: 1024 * 1024,
        }
    }
}

/// Validated runtime-wide configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Runtime release identifier.
    pub runtime_version: String,
    /// Compatibility profile used to interpret application behavior.
    pub compatibility_profile: String,
    /// JavaScript engine adapter requested by this runtime.
    pub engine: EngineKind,
    /// Rendering backend requested by this runtime.
    pub renderer: RendererKind,
    /// Minimum diagnostic log level.
    pub log_level: LogLevel,
    /// Whether developer inspection is enabled.
    pub debug_enabled: bool,
    /// Per-runtime and per-application resource ceilings.
    pub resource_budget: ResourceBudget,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            runtime_version: env!("CARGO_PKG_VERSION").to_owned(),
            compatibility_profile: "wechat-public".to_owned(),
            engine: EngineKind::V8,
            renderer: RendererKind::Headless,
            log_level: LogLevel::Info,
            debug_enabled: false,
            resource_budget: ResourceBudget::default(),
        }
    }
}

impl RuntimeConfig {
    /// Validate required identifiers and non-zero resource limits.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidConfiguration`] when an identifier is empty or a limit is zero.
    pub fn validate(&self) -> Result<()> {
        if self.runtime_version.trim().is_empty() {
            return Err(RuntimeError::InvalidConfiguration {
                field: "runtime_version",
                reason: "must not be empty".to_owned(),
            });
        }
        if self.compatibility_profile.trim().is_empty() {
            return Err(RuntimeError::InvalidConfiguration {
                field: "compatibility_profile",
                reason: "must not be empty".to_owned(),
            });
        }
        let limits = [
            ("max_applications", self.resource_budget.max_applications),
            (
                "max_pages_per_application",
                self.resource_budget.max_pages_per_application,
            ),
            (
                "max_storage_entries_per_application",
                self.resource_budget.max_storage_entries_per_application,
            ),
            (
                "max_storage_value_bytes",
                self.resource_budget.max_storage_value_bytes,
            ),
        ];
        for (field, value) in limits {
            if value == 0 {
                return Err(RuntimeError::InvalidConfiguration {
                    field,
                    reason: "must be greater than zero".to_owned(),
                });
            }
        }
        Ok(())
    }
}
