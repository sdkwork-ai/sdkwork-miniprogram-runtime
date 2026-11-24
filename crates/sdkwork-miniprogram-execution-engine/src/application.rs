use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::{ApplicationStorage, Result, RuntimeError};

/// Application package metadata required to create an isolated runtime context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationManifest {
    /// Stable package identifier used for isolation and diagnostics.
    pub app_id: String,
    /// Package version declared by the application project.
    pub version: String,
    /// First page launched when the application starts.
    pub entry_page: String,
    /// Registered application page paths relative to the package root.
    pub pages: Vec<String>,
}

impl ApplicationManifest {
    /// Validate required fields, page paths, uniqueness, and the page-count budget.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidManifest`] when package metadata or page registration is invalid.
    pub fn validate(&self, max_pages: usize) -> Result<()> {
        if self.app_id.trim().is_empty() {
            return Err(RuntimeError::InvalidManifest {
                field: "app_id",
                reason: "must not be empty".to_owned(),
            });
        }
        if self.version.trim().is_empty() {
            return Err(RuntimeError::InvalidManifest {
                field: "version",
                reason: "must not be empty".to_owned(),
            });
        }
        if self.pages.is_empty() || self.pages.len() > max_pages {
            return Err(RuntimeError::InvalidManifest {
                field: "pages",
                reason: format!("must contain between 1 and {max_pages} entries"),
            });
        }
        let mut unique_pages = HashSet::with_capacity(self.pages.len());
        for page in &self.pages {
            if !is_safe_page_path(page) {
                return Err(RuntimeError::InvalidManifest {
                    field: "pages",
                    reason: format!("`{page}` is not a safe relative page path"),
                });
            }
            if !unique_pages.insert(page) {
                return Err(RuntimeError::InvalidManifest {
                    field: "pages",
                    reason: format!("duplicate page `{page}`"),
                });
            }
        }
        if !unique_pages.contains(&self.entry_page) {
            return Err(RuntimeError::InvalidManifest {
                field: "entry_page",
                reason: "must name one of the registered pages".to_owned(),
            });
        }
        Ok(())
    }
}

fn is_safe_page_path(path: &str) -> bool {
    !path.trim().is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && path
            .split(['/', '\\'])
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Observable application lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationState {
    /// Package metadata has been validated and the context is registered.
    Loaded,
    /// The application has been launched and is not currently visible.
    Launched,
    /// The application is currently visible to the host.
    Visible,
    /// The application is temporarily hidden.
    Hidden,
    /// The application was unloaded and its resources have been released.
    Unloaded,
    /// The application context has been destroyed.
    Destroyed,
}

#[derive(Debug)]
pub(crate) struct ApplicationInner {
    pub manifest: ApplicationManifest,
    state: Mutex<ApplicationState>,
    pub storage: ApplicationStorage,
}

/// Handle to one application context. Its storage and lifecycle are isolated from other apps.
#[derive(Debug, Clone)]
pub struct ApplicationHandle {
    pub(crate) inner: Arc<ApplicationInner>,
}

impl ApplicationHandle {
    pub(crate) fn new(manifest: ApplicationManifest, storage: ApplicationStorage) -> Self {
        Self {
            inner: Arc::new(ApplicationInner {
                manifest,
                state: Mutex::new(ApplicationState::Loaded),
                storage,
            }),
        }
    }

    /// Return the stable application identifier.
    #[must_use]
    pub fn app_id(&self) -> &str {
        &self.inner.manifest.app_id
    }

    /// Return validated immutable package metadata.
    #[must_use]
    pub fn manifest(&self) -> &ApplicationManifest {
        &self.inner.manifest
    }

    /// Return the current lifecycle state.
    ///
    /// # Errors
    /// Returns [`RuntimeError::LockPoisoned`] if the lifecycle lock is poisoned.
    pub fn state(&self) -> Result<ApplicationState> {
        self.inner
            .state
            .lock()
            .map(|state| *state)
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })
    }

    /// Return a shared handle to this application's isolated storage namespace.
    #[must_use]
    pub fn storage(&self) -> ApplicationStorage {
        self.inner.storage.clone()
    }

    /// Transition a loaded application to its launched state.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when launch is not valid.
    pub fn launch(&self) -> Result<()> {
        self.transition(
            "launch",
            &[ApplicationState::Loaded],
            ApplicationState::Launched,
        )
    }

    /// Make a launched or hidden application visible.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when show is not valid.
    pub fn show(&self) -> Result<()> {
        self.transition(
            "show",
            &[ApplicationState::Launched, ApplicationState::Hidden],
            ApplicationState::Visible,
        )
    }

    /// Hide a visible application.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when hide is not valid.
    pub fn hide(&self) -> Result<()> {
        self.transition(
            "hide",
            &[ApplicationState::Visible],
            ApplicationState::Hidden,
        )
    }

    /// Reset a launched, visible, or hidden application to its launched state.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when reload is not valid.
    pub fn reload(&self) -> Result<()> {
        self.transition(
            "reload",
            &[
                ApplicationState::Launched,
                ApplicationState::Visible,
                ApplicationState::Hidden,
            ],
            ApplicationState::Launched,
        )
    }

    /// Unload the application and close its private resources.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when unload is not valid.
    pub fn unload(&self) -> Result<()> {
        self.transition(
            "unload",
            &[
                ApplicationState::Loaded,
                ApplicationState::Launched,
                ApplicationState::Visible,
                ApplicationState::Hidden,
            ],
            ApplicationState::Unloaded,
        )
    }

    /// Destroy the application and close its private resources.
    ///
    /// # Errors
    /// Returns [`RuntimeError::InvalidState`] or [`RuntimeError::ApplicationDestroyed`] when destroy is not valid.
    pub fn destroy(&self) -> Result<()> {
        self.transition(
            "destroy",
            &[
                ApplicationState::Loaded,
                ApplicationState::Launched,
                ApplicationState::Visible,
                ApplicationState::Hidden,
                ApplicationState::Unloaded,
            ],
            ApplicationState::Destroyed,
        )
    }

    fn transition(
        &self,
        operation: &'static str,
        allowed: &[ApplicationState],
        next: ApplicationState,
    ) -> Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        if *state == ApplicationState::Destroyed {
            return Err(RuntimeError::ApplicationDestroyed {
                app_id: self.app_id().to_owned(),
            });
        }
        if !allowed.contains(&state) {
            return Err(RuntimeError::InvalidState {
                app_id: self.app_id().to_owned(),
                state: format!("{state:?}"),
                operation,
            });
        }
        if matches!(
            next,
            ApplicationState::Unloaded | ApplicationState::Destroyed
        ) {
            self.inner.storage.close()?;
        }
        *state = next;
        Ok(())
    }
}
