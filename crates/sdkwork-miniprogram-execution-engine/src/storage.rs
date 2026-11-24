use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{Result, RuntimeError};

#[derive(Debug)]
struct StorageState {
    entries: HashMap<String, String>,
    active: bool,
}

#[derive(Debug)]
struct StorageInner {
    state: Mutex<StorageState>,
    app_id: String,
    max_entries: usize,
    max_value_bytes: usize,
}

/// Application-private in-memory key/value storage with per-instance quotas.
#[derive(Debug, Clone)]
pub struct ApplicationStorage {
    inner: Arc<StorageInner>,
}

impl ApplicationStorage {
    pub(crate) fn new(app_id: String, max_entries: usize, max_value_bytes: usize) -> Self {
        Self {
            inner: Arc::new(StorageInner {
                state: Mutex::new(StorageState {
                    entries: HashMap::new(),
                    active: true,
                }),
                app_id,
                max_entries,
                max_value_bytes,
            }),
        }
    }

    /// Read a value from this application's namespace.
    ///
    /// # Errors
    /// Returns an error when the application namespace is closed or its lock is poisoned.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let state = self.lock_state()?;
        Ok(state.entries.get(key).cloned())
    }

    /// Set a value, enforcing the configured entry and value-size budgets.
    ///
    /// # Errors
    /// Returns an error when the namespace is closed, a quota is exceeded, or its lock is poisoned.
    pub fn set(&self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let key = key.into();
        let value = value.into();
        if value.len() > self.inner.max_value_bytes {
            return Err(RuntimeError::InvalidConfiguration {
                field: "storage_value",
                reason: format!("value exceeds {} bytes", self.inner.max_value_bytes),
            });
        }
        let mut state = self.lock_state()?;
        if !state.entries.contains_key(&key) && state.entries.len() >= self.inner.max_entries {
            return Err(RuntimeError::InvalidConfiguration {
                field: "storage_entries",
                reason: format!("entry limit of {} reached", self.inner.max_entries),
            });
        }
        state.entries.insert(key, value);
        Ok(())
    }

    /// Remove one key and return its former value when it existed.
    ///
    /// # Errors
    /// Returns an error when the application namespace is closed or its lock is poisoned.
    pub fn remove(&self, key: &str) -> Result<Option<String>> {
        let mut state = self.lock_state()?;
        Ok(state.entries.remove(key))
    }

    /// Remove every key in this application's namespace.
    ///
    /// # Errors
    /// Returns an error when the application namespace is closed or its lock is poisoned.
    pub fn clear(&self) -> Result<()> {
        let mut state = self.lock_state()?;
        state.entries.clear();
        Ok(())
    }

    /// Return the number of keys in this namespace.
    ///
    /// # Errors
    /// Returns an error when the application namespace is closed or its lock is poisoned.
    pub fn len(&self) -> Result<usize> {
        let state = self.lock_state()?;
        Ok(state.entries.len())
    }

    /// Return whether this namespace has no keys.
    ///
    /// # Errors
    /// Returns an error when the application namespace is closed or its lock is poisoned.
    pub fn is_empty(&self) -> Result<bool> {
        self.len().map(|length| length == 0)
    }

    pub(crate) fn close(&self) -> Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application storage",
            })?;
        state.active = false;
        state.entries.clear();
        Ok(())
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, StorageState>> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application storage",
            })?;
        if !state.active {
            return Err(RuntimeError::ApplicationStorageClosed {
                app_id: self.inner.app_id.clone(),
            });
        }
        Ok(state)
    }
}
