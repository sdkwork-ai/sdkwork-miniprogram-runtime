use std::path::PathBuf;

use thiserror::Error;

/// Result type used by the package loader API.
pub type LoaderResult<T> = Result<T, LoaderError>;

/// Classification of a project validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// The project manifest is malformed or incomplete.
    InvalidManifest,
    /// A page route is not a safe normalized relative path.
    InvalidPagePath,
    /// A required project file does not exist.
    MissingFile,
    /// A project resource resolves outside its application root.
    UnsafePath,
    /// The manifest declares a page route more than once.
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
    /// The requested project root exists but is not a directory.
    NotDirectory(PathBuf),
    #[error("I/O error at {path}: {source}")]
    /// Reading or writing the referenced filesystem path failed.
    Io {
        /// Path where the operation failed.
        path: PathBuf,
        #[source]
        /// Underlying operating-system error.
        source: std::io::Error,
    },
    #[error("project validation failed with {diagnostics_len} diagnostic(s)", diagnostics_len = .diagnostics.len())]
    /// One or more project structure checks failed.
    InvalidProject {
        /// Detailed failures to correct in the project.
        diagnostics: Vec<Diagnostic>,
    },
    #[error("invalid ZIP archive: {0}")]
    /// The input is not a valid supported ZIP archive.
    InvalidArchive(String),
    #[error("project resource limit exceeded: {0}")]
    /// A source project exceeds configured parser bounds.
    ProjectLimit(String),
    #[error("ZIP archive limit exceeded: {0}")]
    /// An archive or an extracted entry exceeds configured resource bounds.
    ArchiveLimit(String),
    #[error("unsafe ZIP entry path: {0}")]
    /// A ZIP entry path is not safe to extract.
    UnsafeArchivePath(String),
    #[error("destination already exists: {0}")]
    /// The extraction destination already contains a filesystem entry.
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
