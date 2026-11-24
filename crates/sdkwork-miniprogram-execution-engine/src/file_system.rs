use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use crate::{Result, RuntimeError};

#[derive(Debug)]
struct FileSystemState {
    files: HashMap<String, Vec<u8>>,
    total_bytes: usize,
    active: bool,
}

#[derive(Debug)]
struct FileSystemInner {
    state: Mutex<FileSystemState>,
    app_id: String,
    max_files: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
}

/// Application-private in-memory files with normalized paths and per-app quotas.
#[derive(Debug, Clone)]
pub struct ApplicationVirtualFileSystem {
    inner: Arc<FileSystemInner>,
}

/// Metadata for one virtual file or inferred directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualFileMetadata {
    /// Normalized path relative to this application's virtual root.
    pub path: String,
    /// File byte length, or zero for an inferred directory.
    pub size_bytes: usize,
    /// Whether the path is an inferred directory.
    pub is_directory: bool,
}

/// One immediate child returned by [`ApplicationVirtualFileSystem::list_directory`].
pub type VirtualDirectoryEntry = VirtualFileMetadata;

impl ApplicationVirtualFileSystem {
    pub(crate) fn new(
        app_id: String,
        max_files: usize,
        max_file_bytes: usize,
        max_total_bytes: usize,
    ) -> Self {
        Self {
            inner: Arc::new(FileSystemInner {
                state: Mutex::new(FileSystemState {
                    files: HashMap::new(),
                    total_bytes: 0,
                    active: true,
                }),
                app_id,
                max_files,
                max_file_bytes,
                max_total_bytes,
            }),
        }
    }

    /// Read a file from this application's private namespace.
    ///
    /// # Errors
    /// Returns an error for an unsafe path, a closed namespace, or a poisoned lock.
    pub fn read_file(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let path = self.validate_path(path, false)?;
        let state = self.lock_state()?;
        Ok(state.files.get(&path).cloned())
    }

    /// Write or replace a file while enforcing per-file, total-byte and file-count limits.
    ///
    /// A failed write leaves the existing file content and quota accounting unchanged.
    ///
    /// # Errors
    /// Returns an error for an unsafe path, a conflicting file/directory path, an exceeded
    /// quota, a closed namespace, or a poisoned lock.
    pub fn write_file(&self, path: &str, contents: impl AsRef<[u8]>) -> Result<()> {
        let path = self.validate_path(path, false)?;
        let contents = contents.as_ref();
        if contents.len() > self.inner.max_file_bytes {
            return Err(self.quota_error(
                "max_file_bytes",
                self.inner.max_file_bytes,
                contents.len(),
            ));
        }

        let mut state = self.lock_state()?;
        validate_no_path_conflict(&self.inner.app_id, &state.files, &path)?;
        let previous_size = state.files.get(&path).map_or(0, Vec::len);
        if !state.files.contains_key(&path) && state.files.len() >= self.inner.max_files {
            return Err(self.quota_error(
                "max_files_per_application",
                self.inner.max_files,
                state.files.len().saturating_add(1),
            ));
        }
        let resulting_total = state
            .total_bytes
            .saturating_sub(previous_size)
            .saturating_add(contents.len());
        if resulting_total > self.inner.max_total_bytes {
            return Err(self.quota_error(
                "max_total_file_bytes",
                self.inner.max_total_bytes,
                resulting_total,
            ));
        }

        state.files.insert(path, contents.to_vec());
        state.total_bytes = resulting_total;
        Ok(())
    }

    /// Return metadata for a file or an inferred directory when it exists.
    ///
    /// # Errors
    /// Returns an error for an unsafe path, a closed namespace, or a poisoned lock.
    pub fn metadata(&self, path: &str) -> Result<Option<VirtualFileMetadata>> {
        let path = self.validate_path(path, true)?;
        let state = self.lock_state()?;
        if let Some(contents) = state.files.get(&path) {
            return Ok(Some(VirtualFileMetadata {
                path,
                size_bytes: contents.len(),
                is_directory: false,
            }));
        }
        let prefix = directory_prefix(&path);
        let exists = state.files.keys().any(|file| file.starts_with(&prefix));
        Ok(exists.then_some(VirtualFileMetadata {
            path,
            size_bytes: 0,
            is_directory: true,
        }))
    }

    /// Remove one file and return its former bytes when it existed.
    ///
    /// Directories are inferred from their children and cannot be removed as files.
    ///
    /// # Errors
    /// Returns an error for an unsafe path, a closed namespace, or a poisoned lock.
    pub fn remove_file(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let path = self.validate_path(path, false)?;
        let mut state = self.lock_state()?;
        let removed = state.files.remove(&path);
        if let Some(contents) = &removed {
            state.total_bytes = state.total_bytes.saturating_sub(contents.len());
        }
        Ok(removed)
    }

    /// List the immediate file and directory children of a virtual directory.
    ///
    /// The empty path lists the virtual root. Entries are sorted by normalized child path.
    /// A missing directory returns an empty list.
    ///
    /// # Errors
    /// Returns an error for an unsafe path, a closed namespace, or a poisoned lock.
    pub fn list_directory(&self, path: &str) -> Result<Vec<VirtualDirectoryEntry>> {
        let directory = self.validate_path(path, true)?;
        let prefix = directory_prefix(&directory);
        let state = self.lock_state()?;
        let mut children = BTreeMap::<String, VirtualDirectoryEntry>::new();
        for file in state.files.keys().filter(|file| file.starts_with(&prefix)) {
            let remainder = &file[prefix.len()..];
            let child_name = remainder.split('/').next().unwrap_or_default();
            if child_name.is_empty() {
                continue;
            }
            let child_path = if directory.is_empty() {
                child_name.to_owned()
            } else {
                format!("{directory}/{child_name}")
            };
            let is_directory = remainder.contains('/');
            let size_bytes = if is_directory {
                0
            } else {
                state.files.get(file).map_or(0, Vec::len)
            };
            children
                .entry(child_name.to_owned())
                .or_insert(VirtualDirectoryEntry {
                    path: child_path,
                    size_bytes,
                    is_directory,
                });
        }
        Ok(children.into_values().collect())
    }

    /// Return the number of stored files in this namespace.
    ///
    /// # Errors
    /// Returns an error when the namespace is closed or its lock is poisoned.
    pub fn file_count(&self) -> Result<usize> {
        Ok(self.lock_state()?.files.len())
    }

    /// Return the total number of bytes currently stored in this namespace.
    ///
    /// # Errors
    /// Returns an error when the namespace is closed or its lock is poisoned.
    pub fn total_bytes(&self) -> Result<usize> {
        Ok(self.lock_state()?.total_bytes)
    }

    pub(crate) fn close(&self) -> Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application virtual filesystem",
            })?;
        state.active = false;
        state.files.clear();
        state.total_bytes = 0;
        Ok(())
    }

    fn validate_path(&self, path: &str, allow_root: bool) -> Result<String> {
        let valid = if allow_root && path.is_empty() {
            true
        } else {
            !path.is_empty()
                && !path.starts_with('/')
                && !path.starts_with('\\')
                && !path.contains(['\\', ':', '\0'])
                && path
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
        };
        if valid {
            Ok(path.to_owned())
        } else {
            Err(RuntimeError::InvalidFileSystemPath {
                app_id: self.inner.app_id.clone(),
                path: path.to_owned(),
            })
        }
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, FileSystemState>> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned {
                resource: "application virtual filesystem",
            })?;
        if !state.active {
            return Err(RuntimeError::ApplicationFileSystemClosed {
                app_id: self.inner.app_id.clone(),
            });
        }
        Ok(state)
    }

    fn quota_error(&self, quota: &'static str, limit: usize, attempted: usize) -> RuntimeError {
        RuntimeError::FileSystemQuotaExceeded {
            app_id: self.inner.app_id.clone(),
            quota,
            limit,
            attempted,
        }
    }
}

fn directory_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn validate_no_path_conflict(
    app_id: &str,
    files: &HashMap<String, Vec<u8>>,
    path: &str,
) -> Result<()> {
    let mut parent_end = 0;
    while let Some(separator) = path[parent_end..].find('/') {
        parent_end += separator;
        let parent = &path[..parent_end];
        if files.contains_key(parent) {
            return Err(RuntimeError::InvalidFileSystemPath {
                app_id: app_id.to_owned(),
                path: path.to_owned(),
            });
        }
        parent_end += 1;
    }
    let prefix = directory_prefix(path);
    if files.keys().any(|existing| existing.starts_with(&prefix)) {
        return Err(RuntimeError::InvalidFileSystemPath {
            app_id: app_id.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ApplicationVirtualFileSystem;
    use crate::RuntimeError;

    fn file_system(
        max_files: usize,
        max_file_bytes: usize,
        max_total_bytes: usize,
    ) -> ApplicationVirtualFileSystem {
        ApplicationVirtualFileSystem::new(
            "test-app".to_owned(),
            max_files,
            max_file_bytes,
            max_total_bytes,
        )
    }

    #[test]
    fn writes_reads_lists_and_removes_files_with_inferred_directories()
    -> Result<(), Box<dyn std::error::Error>> {
        let file_system = file_system(8, 32, 64);
        file_system.write_file("data/profile.json", br#"{"name":"Ada"}"#)?;
        file_system.write_file("data/settings.json", b"{}")?;
        file_system.write_file("root.txt", b"ok")?;

        assert_eq!(
            file_system.read_file("data/profile.json")?,
            Some(br#"{"name":"Ada"}"#.to_vec())
        );
        assert_eq!(file_system.total_bytes()?, 18);
        let metadata = file_system.metadata("data")?.ok_or("missing directory")?;
        assert!(metadata.is_directory);
        assert_eq!(metadata.size_bytes, 0);
        let entries = file_system.list_directory("")?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "data");
        assert!(entries[0].is_directory);
        assert_eq!(file_system.list_directory("data")?.len(), 2);
        assert_eq!(
            file_system.remove_file("data/settings.json")?,
            Some(b"{}".to_vec())
        );
        assert_eq!(file_system.total_bytes()?, 16);
        Ok(())
    }

    #[test]
    fn rejects_traversal_absolute_and_non_normalized_paths() {
        let file_system = file_system(8, 32, 64);
        for path in [
            "../outside",
            "/absolute",
            "a//b",
            "a/./b",
            "a/../b",
            "C:/temp",
            "a\\b",
            "",
        ] {
            assert!(
                matches!(
                    file_system.write_file(path, b"x"),
                    Err(RuntimeError::InvalidFileSystemPath { .. })
                ),
                "path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn rejects_file_directory_collisions_without_mutating_existing_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let file_system = file_system(8, 32, 64);
        file_system.write_file("data", b"file")?;
        assert!(matches!(
            file_system.write_file("data/child", b"x"),
            Err(RuntimeError::InvalidFileSystemPath { .. })
        ));
        assert_eq!(file_system.read_file("data")?, Some(b"file".to_vec()));
        file_system.write_file("other/child", b"x")?;
        assert!(matches!(
            file_system.write_file("other", b"file"),
            Err(RuntimeError::InvalidFileSystemPath { .. })
        ));
        Ok(())
    }

    #[test]
    fn enforces_file_count_per_file_and_total_byte_quotas_atomically()
    -> Result<(), Box<dyn std::error::Error>> {
        let file_system = file_system(1, 4, 5);
        file_system.write_file("one", b"1234")?;
        assert!(matches!(
            file_system.write_file("one", b"12345"),
            Err(RuntimeError::FileSystemQuotaExceeded {
                quota: "max_file_bytes",
                ..
            })
        ));
        assert!(matches!(
            file_system.write_file("two", b"x"),
            Err(RuntimeError::FileSystemQuotaExceeded {
                quota: "max_files_per_application",
                ..
            })
        ));
        assert_eq!(file_system.read_file("one")?, Some(b"1234".to_vec()));
        Ok(())
    }

    #[test]
    fn rejects_aggregate_quota_excess_and_preserves_prior_contents()
    -> Result<(), Box<dyn std::error::Error>> {
        let file_system = file_system(2, 4, 5);
        file_system.write_file("first.bin", b"1234")?;
        assert!(matches!(
            file_system.write_file("second.bin", b"xy"),
            Err(RuntimeError::FileSystemQuotaExceeded {
                quota: "max_total_file_bytes",
                limit: 5,
                attempted: 6,
                ..
            })
        ));
        assert_eq!(file_system.read_file("second.bin")?, None);
        assert_eq!(file_system.total_bytes()?, 4);
        file_system.write_file("first.bin", b"123")?;
        file_system.write_file("second.bin", b"xy")?;
        assert_eq!(file_system.total_bytes()?, 5);
        Ok(())
    }

    #[test]
    fn close_clears_files_and_blocks_existing_handles() -> Result<(), Box<dyn std::error::Error>> {
        let file_system = file_system(2, 8, 16);
        file_system.write_file("data", b"private")?;
        file_system.close()?;
        assert!(matches!(
            file_system.read_file("data"),
            Err(RuntimeError::ApplicationFileSystemClosed { .. })
        ));
        Ok(())
    }
}
