use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

use crate::{
    ApplicationHandle, ApplicationManifest, ApplicationStorage, RendererKind, Result,
    RuntimeConfig, RuntimeError,
};

#[derive(Debug)]
struct RuntimeInner {
    config: RuntimeConfig,
    applications: Mutex<HashMap<String, ApplicationHandle>>,
}

/// A configured runtime instance with isolated application contexts.
#[derive(Debug, Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

/// Builder validates the runtime configuration before creating any instance state.
#[must_use]
#[derive(Debug, Clone)]
pub struct RuntimeBuilder {
    config: RuntimeConfig,
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeBuilder {
    /// Create a builder with the default profile, engine, renderer, and resource budget.
    pub fn new() -> Self {
        Self {
            config: RuntimeConfig::default(),
        }
    }

    /// Replace the runtime configuration.
    pub fn config(mut self, config: RuntimeConfig) -> Self {
        self.config = config;
        self
    }

    /// Select a rendering backend.
    pub fn renderer(mut self, renderer: RendererKind) -> Self {
        self.config.renderer = renderer;
        self
    }

    /// Validate the configuration and construct an isolated runtime instance.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidConfiguration`] if the configuration is invalid.
    pub fn build(self) -> Result<Runtime> {
        self.config.validate()?;
        info!(
            runtime_version = %self.config.runtime_version,
            compatibility_profile = %self.config.compatibility_profile,
            renderer = ?self.config.renderer,
            "runtime created"
        );
        Ok(Runtime {
            inner: Arc::new(RuntimeInner {
                config: self.config,
                applications: Mutex::new(HashMap::new()),
            }),
        })
    }
}

impl Runtime {
    /// Return the validated configuration used by this runtime.
    #[must_use]
    pub fn config(&self) -> &RuntimeConfig {
        &self.inner.config
    }

    /// Validate and register an application with a new private context and storage namespace.
    ///
    /// # Errors
    /// Returns an error when the manifest is invalid, the app id is already loaded, the application budget is exhausted, or the registry lock is poisoned.
    pub fn load_application(&self, manifest: ApplicationManifest) -> Result<ApplicationHandle> {
        manifest.validate(self.inner.config.resource_budget.max_pages_per_application)?;
        let mut applications =
            self.inner
                .applications
                .lock()
                .map_err(|_| RuntimeError::LockPoisoned {
                    resource: "runtime application registry",
                })?;
        if applications.contains_key(&manifest.app_id) {
            return Err(RuntimeError::ApplicationAlreadyLoaded {
                app_id: manifest.app_id,
            });
        }
        if applications.len() >= self.inner.config.resource_budget.max_applications {
            return Err(RuntimeError::InvalidConfiguration {
                field: "max_applications",
                reason: format!(
                    "application limit of {} reached",
                    self.inner.config.resource_budget.max_applications
                ),
            });
        }
        let app_id = manifest.app_id.clone();
        let page_count = manifest.pages.len();
        let storage = ApplicationStorage::new(
            manifest.app_id.clone(),
            self.inner
                .config
                .resource_budget
                .max_storage_entries_per_application,
            self.inner.config.resource_budget.max_storage_value_bytes,
        );
        let handle = ApplicationHandle::new(
            manifest,
            storage,
            self.inner.config.resource_budget.max_pages_per_application,
            self.inner
                .config
                .resource_budget
                .max_javascript_source_bytes,
            self.inner.config.resource_budget.max_javascript_heap_bytes,
            Duration::from_millis(self.inner.config.resource_budget.javascript_timeout_ms),
        );
        applications.insert(app_id, handle.clone());
        info!(app_id = %handle.app_id(), page_count, "application loaded");
        Ok(handle)
    }

    /// Look up a loaded application by its package identifier.
    ///
    /// # Errors
    /// Returns [`RuntimeError::LockPoisoned`] if the application registry lock is poisoned.
    pub fn application(&self, app_id: &str) -> Result<Option<ApplicationHandle>> {
        let applications =
            self.inner
                .applications
                .lock()
                .map_err(|_| RuntimeError::LockPoisoned {
                    resource: "runtime application registry",
                })?;
        Ok(applications.get(app_id).cloned())
    }

    /// Unload an application and release its private storage namespace.
    ///
    /// # Errors
    /// Returns an error when the app id is not loaded, the lifecycle transition is invalid, or a runtime lock is poisoned.
    pub fn unload_application(&self, app_id: &str) -> Result<()> {
        self.transition_and_remove(app_id, ApplicationHandle::unload)
    }

    /// Destroy an application and release its private storage namespace.
    ///
    /// # Errors
    /// Returns an error when the app id is not loaded, the lifecycle transition is invalid, or a runtime lock is poisoned.
    pub fn destroy_application(&self, app_id: &str) -> Result<()> {
        self.transition_and_remove(app_id, ApplicationHandle::destroy)
    }

    /// Return the number of applications currently registered in this runtime.
    ///
    /// # Errors
    /// Returns [`RuntimeError::LockPoisoned`] if the application registry lock is poisoned.
    pub fn application_count(&self) -> Result<usize> {
        let applications =
            self.inner
                .applications
                .lock()
                .map_err(|_| RuntimeError::LockPoisoned {
                    resource: "runtime application registry",
                })?;
        Ok(applications.len())
    }

    fn transition_and_remove(
        &self,
        app_id: &str,
        transition: fn(&ApplicationHandle) -> Result<()>,
    ) -> Result<()> {
        let mut applications =
            self.inner
                .applications
                .lock()
                .map_err(|_| RuntimeError::LockPoisoned {
                    resource: "runtime application registry",
                })?;
        let handle = applications
            .get(app_id)
            .ok_or_else(|| RuntimeError::ApplicationNotFound {
                app_id: app_id.to_owned(),
            })?;
        transition(handle)?;
        applications.remove(app_id);
        info!(app_id, "application removed from runtime");
        Ok(())
    }
}
