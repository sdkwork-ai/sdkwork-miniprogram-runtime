//! Source project discovery and secure ZIP extraction for mini program packages.

mod archive;
mod error;
mod project;

pub use archive::{ArchiveLimits, extract_zip};
pub use error::{Diagnostic, DiagnosticKind, LoaderError, LoaderResult};
pub use project::{MiniProgramProject, PageDefinition, ProjectManifest, load_project};
