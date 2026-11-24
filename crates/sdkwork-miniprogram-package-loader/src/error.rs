use std::path::PathBuf;

use thiserror::Error;

pub type LoaderResult<T> = Result<T, LoaderError>;

/// Classification of a project validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    InvalidManifest,
    InvalidPagePath,
    MissingFile,
    UnsafePath,
    DuplicatePage,
}

/// File-scoped validation finding with an optional source position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Finding category.
    pub kind: DiagnosticKind,
    /// Project-relative source path.
    pub path: PathBuf,
    /// Human-readable details.
    pub message: String,
    /// One-based source line when available.
    pub line: Option<usize>,
    /// One-based source column when available.
    pub column: Option<usize>,
}

/// Structured failure while inspecting a project or extracting an archive.
#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("project root is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("project validation failed with {diagnostics_len} diagnostic(s)", diagnostics_len = .diagnostics.len())]
    InvalidProject { diagnostics: Vec<Diagnostic> },
    #[error("invalid ZIP archive: {0}")]
    InvalidArchive(String),
    #[error("project resource limit exceeded: {0}")]
    ProjectLimit(String),
    #[error("ZIP archive limit exceeded: {0}")]
    ArchiveLimit(String),
    #[error("unsafe ZIP entry path: {0}")]
    UnsafeArchivePath(String),
    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),
}

impl LoaderError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
