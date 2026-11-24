use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zip::ZipArchive;

use crate::error::{LoaderError, LoaderResult};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 128 * 1024 * 1024,
            max_entries: 20_000,
            max_file_bytes: 64 * 1024 * 1024,
            max_total_uncompressed_bytes: 512 * 1024 * 1024,
        }
    }
}

pub fn extract_zip(
    archive_path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    limits: ArchiveLimits,
) -> LoaderResult<()> {
    let archive_path = archive_path.as_ref();
    let destination = destination.as_ref();
    let metadata =
        fs::metadata(archive_path).map_err(|error| LoaderError::io(archive_path, error))?;
    if metadata.len() > limits.max_archive_bytes {
        return Err(LoaderError::ArchiveLimit(format!(
            "compressed archive is {} bytes, limit is {}",
            metadata.len(),
            limits.max_archive_bytes
        )));
    }
    if destination.exists() {
        return Err(LoaderError::DestinationExists(destination.to_path_buf()));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| LoaderError::io(parent, error))?;
    let name = destination
        .file_name()
        .ok_or_else(|| LoaderError::UnsafeArchivePath(destination.display().to_string()))?;
    let destination = parent.join(name);

    let mut archive = ZipArchive::new(
        File::open(archive_path).map_err(|error| LoaderError::io(archive_path, error))?,
    )
    .map_err(|error| LoaderError::InvalidArchive(error.to_string()))?;
    if archive.len() > limits.max_entries {
        return Err(LoaderError::ArchiveLimit(format!(
            "archive contains {} entries, limit is {}",
            archive.len(),
            limits.max_entries
        )));
    }

    let staging = create_staging_directory(&parent, name)?;
    let result = extract_entries(&mut archive, &staging, limits).and_then(|()| {
        fs::rename(&staging, &destination).map_err(|error| LoaderError::io(&destination, error))
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn extract_entries<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    staging: &Path,
    limits: ArchiveLimits,
) -> LoaderResult<()> {
    let mut seen_paths = HashSet::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| LoaderError::InvalidArchive(error.to_string()))?;
        let raw_name = entry.name().to_owned();
        let relative_path = validate_archive_path(&raw_name)?;
        let normalized = relative_path
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        if !seen_paths.insert(normalized) {
            return Err(LoaderError::UnsafeArchivePath(format!(
                "duplicate path after normalization: {raw_name}"
            )));
        }
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(LoaderError::UnsafeArchivePath(format!(
                    "symbolic links are not allowed: {raw_name}"
                )));
            }
        }
        if entry.is_dir() {
            fs::create_dir_all(staging.join(relative_path))
                .map_err(|error| LoaderError::io(staging, error))?;
            continue;
        }
        let declared_size = entry.size();
        if declared_size > limits.max_file_bytes {
            return Err(LoaderError::ArchiveLimit(format!(
                "entry {raw_name} is {declared_size} bytes, per-file limit is {}",
                limits.max_file_bytes
            )));
        }
        total_uncompressed = total_uncompressed
            .checked_add(declared_size)
            .ok_or_else(|| LoaderError::ArchiveLimit("uncompressed size overflow".to_owned()))?;
        if total_uncompressed > limits.max_total_uncompressed_bytes {
            return Err(LoaderError::ArchiveLimit(format!(
                "uncompressed archive exceeds {} bytes",
                limits.max_total_uncompressed_bytes
            )));
        }
        let output_path = staging.join(&relative_path);
        let parent = output_path
            .parent()
            .ok_or_else(|| LoaderError::UnsafeArchivePath(raw_name.clone()))?;
        fs::create_dir_all(parent).map_err(|error| LoaderError::io(parent, error))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|error| LoaderError::io(&output_path, error))?;
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let count = entry
                .read(&mut buffer)
                .map_err(|error| LoaderError::InvalidArchive(error.to_string()))?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .ok_or_else(|| LoaderError::ArchiveLimit("entry size overflow".to_owned()))?;
            if copied > limits.max_file_bytes || copied > declared_size {
                return Err(LoaderError::ArchiveLimit(format!(
                    "entry {raw_name} expands beyond its declared or configured limit"
                )));
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| LoaderError::io(&output_path, error))?;
        }
        if copied != declared_size {
            return Err(LoaderError::InvalidArchive(format!(
                "entry {raw_name} expanded to {copied} bytes but declared {declared_size}"
            )));
        }
        output
            .flush()
            .map_err(|error| LoaderError::io(&output_path, error))?;
    }
    Ok(())
}

fn create_staging_directory(
    parent: &Path,
    destination_name: &std::ffi::OsStr,
) -> LoaderResult<PathBuf> {
    let destination_name = destination_name.to_string_lossy();
    for _ in 0..32 {
        let counter = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{destination_name}.extract-{}-{counter}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(LoaderError::io(path, error)),
        }
    }
    Err(LoaderError::ArchiveLimit(
        "could not allocate a unique extraction staging directory".to_owned(),
    ))
}

fn validate_archive_path(raw_name: &str) -> LoaderResult<PathBuf> {
    if raw_name.is_empty()
        || raw_name.starts_with('/')
        || raw_name.starts_with('\\')
        || raw_name.contains('\\')
        || raw_name.contains(':')
        || raw_name.contains('\0')
    {
        return Err(LoaderError::UnsafeArchivePath(raw_name.to_owned()));
    }
    let normalized = raw_name.trim_end_matches('/');
    if normalized.is_empty()
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(LoaderError::UnsafeArchivePath(raw_name.to_owned()));
    }
    let path = Path::new(normalized);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(LoaderError::UnsafeArchivePath(raw_name.to_owned()));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    use zip::write::SimpleFileOptions;

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sdkwork-zip-{name}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn create_archive(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), Box<dyn Error>> {
        let file = File::create(path)?;
        let mut writer = zip::ZipWriter::new(file);
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default())?;
            writer.write_all(content)?;
        }
        writer.finish()?;
        Ok(())
    }

    #[test]
    fn extracts_valid_archive_with_resource_limits() -> Result<(), Box<dyn Error>> {
        let temporary = temporary_directory("valid");
        fs::create_dir_all(&temporary)?;
        let archive = temporary.join("valid.zip");
        let destination = temporary.join("unpacked");
        create_archive(
            &archive,
            &[("app.json", b"{\"pages\":[]}"), ("pages/home.wxml", b"ok")],
        )?;

        extract_zip(&archive, &destination, ArchiveLimits::default())?;
        assert_eq!(fs::read(destination.join("pages/home.wxml"))?, b"ok");

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn rejects_traversal_before_writing_outside_destination() -> Result<(), Box<dyn Error>> {
        let temporary = temporary_directory("slip");
        fs::create_dir_all(&temporary)?;
        let archive = temporary.join("unsafe.zip");
        let destination = temporary.join("unpacked");
        create_archive(&archive, &[("../outside.txt", b"bad")])?;

        let result = extract_zip(&archive, &destination, ArchiveLimits::default());
        assert!(matches!(result, Err(LoaderError::UnsafeArchivePath(_))));
        assert!(!temporary.join("outside.txt").exists());
        assert!(!destination.exists());

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn enforces_entry_and_expansion_limits() -> Result<(), Box<dyn Error>> {
        let temporary = temporary_directory("limit");
        fs::create_dir_all(&temporary)?;
        let archive = temporary.join("large.zip");
        let destination = temporary.join("unpacked");
        create_archive(&archive, &[("payload.bin", b"123456789")])?;
        let limits = ArchiveLimits {
            max_file_bytes: 8,
            max_total_uncompressed_bytes: 8,
            ..ArchiveLimits::default()
        };

        assert!(matches!(
            extract_zip(&archive, &destination, limits),
            Err(LoaderError::ArchiveLimit(_))
        ));
        assert!(!destination.exists());

        fs::remove_dir_all(temporary)?;
        Ok(())
    }
}
