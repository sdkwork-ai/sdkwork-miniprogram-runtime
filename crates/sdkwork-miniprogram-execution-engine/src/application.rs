use sdkwork_miniprogram_v8_adapter::JavaScriptContext;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

use crate::{ApplicationStorage, ApplicationVirtualFileSystem, Result, RuntimeError};

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
        let entry_page = self.entry_page.replace('\\', "/");
        if unique_pages
            .iter()
            .all(|page| page.replace('\\', "/") != entry_page)
        {
            return Err(RuntimeError::InvalidManifest {
                field: "entry_page",
                reason: "must use the same normalized route as a registered page".to_owned(),
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
    state: Mutex<ApplicationRuntimeState>,
    pub storage: ApplicationStorage,
    pub file_system: ApplicationVirtualFileSystem,
    max_page_stack_depth: usize,
    max_javascript_source_bytes: usize,
    max_javascript_heap_bytes: usize,
    javascript_timeout: Duration,
}

#[derive(Debug)]
struct ApplicationRuntimeState {
    lifecycle: ApplicationState,
    page_stack: Option<Vec<String>>,
    javascript: Option<JavaScriptContext>,
}

/// Handle to one application context. Its storage and lifecycle are isolated from other apps.
#[derive(Debug, Clone)]
pub struct ApplicationHandle {
    pub(crate) inner: Arc<ApplicationInner>,
}

impl ApplicationHandle {
    pub(crate) fn new(
        manifest: ApplicationManifest,
        storage: ApplicationStorage,
        file_system: ApplicationVirtualFileSystem,
        max_page_stack_depth: usize,
        max_javascript_source_bytes: usize,
        max_javascript_heap_bytes: usize,
        javascript_timeout: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(ApplicationInner {
                state: Mutex::new(ApplicationRuntimeState {
                    lifecycle: ApplicationState::Loaded,
                    page_stack: None,
                    javascript: None,
                }),
                manifest,
                storage,
                file_system,
                max_page_stack_depth,
                max_javascript_source_bytes,
                max_javascript_heap_bytes,
                javascript_timeout,
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
            .map(|state| state.lifecycle)
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })
    }

    /// Return a shared handle to this application's isolated storage namespace.
    #[must_use]
    pub fn storage(&self) -> ApplicationStorage {
        self.inner.storage.clone()
    }

    /// Return this application's isolated in-memory virtual filesystem handle.
    #[must_use]
    pub fn file_system(&self) -> ApplicationVirtualFileSystem {
        self.inner.file_system.clone()
    }

    /// Evaluate JavaScript in this application's persistent, isolated V8 context.
    ///
    /// The context is created on first use and remains available through reloads. It is
    /// released when the application is unloaded or destroyed.
    ///
    /// # Errors
    /// Returns an error when the app is not active, V8 initialization/evaluation fails,
    /// the execution deadline expires, or the application state lock is poisoned.
    pub fn evaluate_javascript(&self, source: &str) -> Result<String> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        if !matches!(
            state.lifecycle,
            ApplicationState::Loaded
                | ApplicationState::Launched
                | ApplicationState::Visible
                | ApplicationState::Hidden
        ) {
            return Err(RuntimeError::InvalidState {
                app_id: self.app_id().to_owned(),
                state: format!("{:?}", state.lifecycle),
                operation: "evaluate_javascript",
            });
        }
        let context = if let Some(context) = &mut state.javascript {
            context
        } else {
            let context = JavaScriptContext::with_options(
                self.inner.max_javascript_source_bytes,
                self.inner.max_javascript_heap_bytes,
                self.inner.javascript_timeout,
            )
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })?;
            state.javascript.insert(context)
        };
        context
            .evaluate(source)
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })
    }

    /// Register an application or page source file in this application's persistent JS context.
    ///
    /// # Errors
    /// Returns a structured JS execution error or an invalid lifecycle error.
    pub fn register_javascript_source(&self, source_name: &str, source: &str) -> Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        if !matches!(
            state.lifecycle,
            ApplicationState::Loaded
                | ApplicationState::Launched
                | ApplicationState::Visible
                | ApplicationState::Hidden
        ) {
            return Err(RuntimeError::InvalidState {
                app_id: self.app_id().to_owned(),
                state: format!("{:?}", state.lifecycle),
                operation: "register_javascript_source",
            });
        }
        let context = if let Some(context) = &mut state.javascript {
            context
        } else {
            let context = JavaScriptContext::with_options(
                self.inner.max_javascript_source_bytes,
                self.inner.max_javascript_heap_bytes,
                self.inner.javascript_timeout,
            )
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })?;
            state.javascript.insert(context)
        };
        context
            .register_source(source_name, source)
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })
    }

    /// Return a JSON snapshot of registration definitions created by app and page scripts.
    ///
    /// # Errors
    /// Returns a structured JS execution error or an invalid lifecycle error.
    pub fn javascript_registration_snapshot(&self) -> Result<serde_json::Value> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        let context =
            state
                .javascript
                .as_mut()
                .ok_or_else(|| RuntimeError::JavaScriptExecution {
                    app_id: self.app_id().to_owned(),
                    reason: sdkwork_miniprogram_v8_adapter::V8Error::WorkerStopped,
                })?;
        context
            .registration_snapshot()
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })
    }

    /// Invoke an application lifecycle method such as `onLaunch` or `onShow` when registered.
    ///
    /// # Errors
    /// Returns an error when no application script context exists or callback execution fails.
    pub fn invoke_app_lifecycle(&self, method: &str) -> Result<Option<String>> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        let context =
            state
                .javascript
                .as_mut()
                .ok_or_else(|| RuntimeError::JavaScriptExecution {
                    app_id: self.app_id().to_owned(),
                    reason: sdkwork_miniprogram_v8_adapter::V8Error::WorkerStopped,
                })?;
        context
            .invoke_app_callback(method)
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })
    }

    /// Invoke a registered page lifecycle method such as `onLoad`.
    ///
    /// # Errors
    /// Returns an error when the page index is missing, no script context exists or callback fails.
    pub fn invoke_page_lifecycle(&self, page_index: usize, method: &str) -> Result<Option<String>> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        let context =
            state
                .javascript
                .as_mut()
                .ok_or_else(|| RuntimeError::JavaScriptExecution {
                    app_id: self.app_id().to_owned(),
                    reason: sdkwork_miniprogram_v8_adapter::V8Error::WorkerStopped,
                })?;
        context
            .invoke_page_callback(page_index, method)
            .map_err(|reason| RuntimeError::JavaScriptExecution {
                app_id: self.app_id().to_owned(),
                reason,
            })
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

    /// Return a snapshot of the current registered page stack.
    ///
    /// # Errors
    /// Returns [`RuntimeError::LockPoisoned`] if the application state lock is poisoned.
    pub fn page_stack(&self) -> Result<Vec<String>> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        Ok(state.page_stack.clone().unwrap_or_default())
    }

    /// Push a registered page onto this application's page stack.
    ///
    /// The stack is bounded by the runtime's `max_pages_per_application` resource limit.
    ///
    /// # Errors
    /// Returns an error when the route is unregistered, navigation is not available in the
    /// current lifecycle state, the stack limit is reached, or the state lock is poisoned.
    pub fn navigate_to(&self, route: &str) -> Result<String> {
        self.navigate(
            route,
            self.inner.max_page_stack_depth,
            NavigationOperation::Push,
        )
    }

    /// Replace the current page with another registered page, preserving pages beneath it.
    ///
    /// # Errors
    /// Returns an error when the route is unregistered, navigation is not available in the
    /// current lifecycle state, the stack is empty, or the state lock is poisoned.
    pub fn redirect_to(&self, route: &str) -> Result<String> {
        self.navigate(route, 0, NavigationOperation::Replace)
    }

    /// Pop `delta` pages and return the newly current route.
    ///
    /// # Errors
    /// Returns an error for a zero delta, an unregistered lifecycle state, a delta that exceeds
    /// the available back stack, or a poisoned state lock.
    pub fn navigate_back(&self, delta: usize) -> Result<String> {
        let mut state = self.lock_navigable_state("navigate_back")?;
        if delta == 0 {
            return Err(RuntimeError::NavigationFailed {
                app_id: self.app_id().to_owned(),
                reason: NavigationFailure::InvalidDelta,
            });
        }
        let page_stack =
            state
                .page_stack
                .as_mut()
                .ok_or_else(|| RuntimeError::NavigationFailed {
                    app_id: self.app_id().to_owned(),
                    reason: NavigationFailure::EmptyPageStack,
                })?;
        let available = page_stack.len().saturating_sub(1);
        if delta > available {
            return Err(RuntimeError::NavigationFailed {
                app_id: self.app_id().to_owned(),
                reason: NavigationFailure::InsufficientPages {
                    requested: delta,
                    available,
                },
            });
        }
        let next_length = page_stack.len() - delta;
        page_stack.truncate(next_length);
        page_stack
            .last()
            .cloned()
            .ok_or_else(|| RuntimeError::NavigationFailed {
                app_id: self.app_id().to_owned(),
                reason: NavigationFailure::InsufficientPages {
                    requested: delta,
                    available,
                },
            })
    }

    fn navigate(
        &self,
        route: &str,
        max_stack_depth: usize,
        operation: NavigationOperation,
    ) -> Result<String> {
        if !self.inner.manifest.pages.iter().any(|page| page == route) {
            return Err(RuntimeError::NavigationFailed {
                app_id: self.app_id().to_owned(),
                reason: NavigationFailure::RouteNotRegistered {
                    route: route.to_owned(),
                },
            });
        }
        let mut state = self.lock_navigable_state(operation.as_str())?;
        let page_stack =
            state
                .page_stack
                .as_mut()
                .ok_or_else(|| RuntimeError::NavigationFailed {
                    app_id: self.app_id().to_owned(),
                    reason: NavigationFailure::EmptyPageStack,
                })?;
        match operation {
            NavigationOperation::Push => {
                if page_stack.len() >= max_stack_depth {
                    return Err(RuntimeError::NavigationFailed {
                        app_id: self.app_id().to_owned(),
                        reason: NavigationFailure::StackLimitReached {
                            limit: max_stack_depth,
                        },
                    });
                }
                page_stack.push(route.to_owned());
            }
            NavigationOperation::Replace => {
                let Some(current) = page_stack.last_mut() else {
                    return Err(RuntimeError::NavigationFailed {
                        app_id: self.app_id().to_owned(),
                        reason: NavigationFailure::EmptyPageStack,
                    });
                };
                route.clone_into(current);
            }
        }
        Ok(route.to_owned())
    }

    fn lock_navigable_state(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, ApplicationRuntimeState>> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application state",
            })?;
        if !matches!(
            state.lifecycle,
            ApplicationState::Launched | ApplicationState::Visible | ApplicationState::Hidden
        ) {
            return Err(RuntimeError::InvalidState {
                app_id: self.app_id().to_owned(),
                state: format!("{:?}", state.lifecycle),
                operation,
            });
        }
        Ok(state)
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
        if state.lifecycle == ApplicationState::Destroyed {
            return Err(RuntimeError::ApplicationDestroyed {
                app_id: self.app_id().to_owned(),
            });
        }
        if !allowed.contains(&state.lifecycle) {
            return Err(RuntimeError::InvalidState {
                app_id: self.app_id().to_owned(),
                state: format!("{:?}", state.lifecycle),
                operation,
            });
        }
        if matches!(
            next,
            ApplicationState::Unloaded | ApplicationState::Destroyed
        ) {
            self.inner.storage.close()?;
            self.inner.file_system.close()?;
            state.page_stack = None;
        } else if next == ApplicationState::Launched {
            state.page_stack = Some(vec![self.inner.manifest.entry_page.clone()]);
        }
        if matches!(
            next,
            ApplicationState::Unloaded | ApplicationState::Destroyed
        ) {
            state.javascript = None;
        }
        state.lifecycle = next;
        info!(
            app_id = %self.app_id(),
            operation,
            state = ?next,
            "application lifecycle transition"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum NavigationOperation {
    Push,
    Replace,
}

impl NavigationOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Push => "navigate_to",
            Self::Replace => "redirect_to",
        }
    }
}

/// A typed explanation for a rejected page-navigation operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationFailure {
    /// The requested route does not appear in the application manifest.
    RouteNotRegistered {
        /// Route requested by the caller.
        route: String,
    },
    /// The configured maximum page-stack depth has been reached.
    StackLimitReached {
        /// Maximum stack depth allowed by this runtime.
        limit: usize,
    },
    /// The requested back-navigation delta cannot be satisfied.
    InsufficientPages {
        /// Number of pages requested to pop.
        requested: usize,
        /// Number of pages currently available to pop.
        available: usize,
    },
    /// Page-stack navigation requires a positive delta.
    InvalidDelta,
    /// The page stack has been cleared or is otherwise empty.
    EmptyPageStack,
}
