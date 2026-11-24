use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::{Diagnostic, DiagnosticKind, LoaderError, LoaderResult};

const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PAGE_COUNT: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// The subset of `app.json` needed to discover an application's page files.
pub struct ProjectManifest {
    /// Top-level page routes.
    pub pages: Vec<String>,
    #[serde(default, alias = "subPackages")]
    /// Optional subpackage declarations.
    pub subpackages: Vec<SubpackageDefinition>,
    #[serde(default)]
    /// Optional initial page route.
    pub entry_page_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Page group rooted in a subpackage directory.
pub struct SubpackageDefinition {
    /// Relative subpackage root.
    pub root: String,
    #[serde(default)]
    /// Routes relative to the subpackage root.
    pub pages: Vec<String>,
}

#[derive(Debug, Clone)]
/// Resolved source files for one page.
pub struct PageDefinition {
    /// Full application route.
    pub route: String,
    /// Canonical JavaScript source path.
    pub script_path: PathBuf,
    /// Canonical WXML source path.
    pub template_path: PathBuf,
    /// Canonical WXSS source path when present.
    pub style_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
/// Validated source project and its resolved page files.
pub struct MiniProgramProject {
    /// Canonical source project root.
    pub root: PathBuf,
    /// Parsed application manifest.
    pub manifest: ProjectManifest,
    /// Page definitions in manifest order.
    pub pages: Vec<PageDefinition>,
    /// Canonical application JavaScript source path.
    pub app_script_path: PathBuf,
    /// Canonical application WXSS source path when present.
    pub app_style_path: Option<PathBuf>,
}

impl MiniProgramProject {
    /// Reads a project resource after resolving and checking its canonical path.
    ///
    /// # Errors
    /// Returns an error when the path is invalid, missing, or resolves outside the project root.
    pub fn read_file(&self, relative: impl AsRef<Path>) -> LoaderResult<Vec<u8>> {
        let relative = relative.as_ref();
        let path = resolve_file(&self.root, relative)
            .map_err(|message| LoaderError::InvalidProject {
                diagnostics: vec![Diagnostic {
                    kind: DiagnosticKind::UnsafePath,
                    path: relative.to_path_buf(),
                    message,
                    line: None,
                    column: None,
                }],
            })?
            .ok_or_else(|| {
                LoaderError::io(
                    self.root.join(relative),
                    std::io::Error::from(std::io::ErrorKind::NotFound),
                )
            })?;
        fs::read(&path).map_err(|error| LoaderError::io(path, error))
    }
}

type ProjectRoutes = Vec<(PathBuf, PathBuf)>;
type ResolvedProjectFiles = (PathBuf, Option<PathBuf>, Vec<PageDefinition>);

/// Parses `app.json`, resolves source files, and rejects resources escaping the root.
///
/// # Errors
/// Returns an error when the root, manifest, declared routes, or required page files are invalid.
pub fn load_project(root: impl AsRef<Path>) -> LoaderResult<MiniProgramProject> {
    let requested_root = root.as_ref();
    let root =
        fs::canonicalize(requested_root).map_err(|error| LoaderError::io(requested_root, error))?;
    if !root.is_dir() {
        return Err(LoaderError::NotDirectory(root));
    }

    let manifest = read_manifest(&root)?;
    let routes = validate_routes(&manifest)?;
    let (app_script_path, app_style_path, pages) = resolve_project_files(&root, routes)?;

    Ok(MiniProgramProject {
        root,
        manifest,
        pages,
        app_script_path,
        app_style_path,
    })
}

fn read_manifest(root: &Path) -> LoaderResult<ProjectManifest> {
    let manifest_relative = Path::new("app.json");
    let manifest_path = resolve_required_file(root, manifest_relative, manifest_relative).map_err(
        |diagnostic| LoaderError::InvalidProject {
            diagnostics: vec![diagnostic],
        },
    )?;
    let manifest_metadata =
        fs::metadata(&manifest_path).map_err(|error| LoaderError::io(&manifest_path, error))?;
    if manifest_metadata.len() > MAX_MANIFEST_BYTES {
        return Err(LoaderError::ProjectLimit(format!(
            "app.json is {} bytes, limit is {MAX_MANIFEST_BYTES}",
            manifest_metadata.len()
        )));
    }
    let manifest_source = fs::read_to_string(&manifest_path)
        .map_err(|error| LoaderError::io(&manifest_path, error))?;
    let manifest = serde_json::from_str::<ProjectManifest>(&manifest_source).map_err(|error| {
        LoaderError::InvalidProject {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::InvalidManifest,
                path: PathBuf::from("app.json"),
                message: error.to_string(),
                line: Some(error.line()),
                column: Some(error.column()),
            }],
        }
    })?;
    Ok(manifest)
}

fn validate_routes(manifest: &ProjectManifest) -> LoaderResult<ProjectRoutes> {
    let page_count = manifest
        .subpackages
        .iter()
        .fold(manifest.pages.len(), |count, subpackage| {
            count.saturating_add(subpackage.pages.len())
        });
    if page_count > MAX_PAGE_COUNT {
        return Err(LoaderError::ProjectLimit(format!(
            "project declares {page_count} pages, limit is {MAX_PAGE_COUNT}"
        )));
    }

    let mut diagnostics = Vec::new();
    let mut routes = Vec::new();
    let mut known_routes = HashSet::new();
    for route in &manifest.pages {
        add_route(
            route,
            Path::new(""),
            &mut known_routes,
            &mut routes,
            &mut diagnostics,
        );
    }
    for subpackage in &manifest.subpackages {
        let package_root = Path::new(&subpackage.root);
        if validate_relative_path(package_root).is_err() {
            diagnostics.push(Diagnostic {
                kind: DiagnosticKind::UnsafePath,
                path: PathBuf::from(&subpackage.root),
                message: "subpackage root must be a normalized relative path".to_owned(),
                line: None,
                column: None,
            });
            continue;
        }
        for route in &subpackage.pages {
            add_route(
                route,
                package_root,
                &mut known_routes,
                &mut routes,
                &mut diagnostics,
            );
        }
    }
    if let Some(entry_page) = &manifest.entry_page_path {
        let entry_key = entry_page.replace('\\', "/").to_lowercase();
        if !known_routes.contains(&entry_key) {
            diagnostics.push(Diagnostic {
                kind: DiagnosticKind::InvalidManifest,
                path: PathBuf::from("app.json"),
                message: format!("entryPagePath `{entry_page}` does not name a registered page"),
                line: None,
                column: None,
            });
        }
    }
    if routes.is_empty() {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::InvalidManifest,
            path: PathBuf::from("app.json"),
            message: "at least one valid page route is required".to_owned(),
            line: None,
            column: None,
        });
    }

    if diagnostics.is_empty() {
        Ok(routes)
    } else {
        Err(LoaderError::InvalidProject { diagnostics })
    }
}

fn resolve_project_files(root: &Path, routes: ProjectRoutes) -> LoaderResult<ResolvedProjectFiles> {
    let app_script = PathBuf::from("app.js");
    let app_script_path = resolve_required_file(root, &app_script, &app_script);
    let app_style_path = resolve_optional_file(root, Path::new("app.wxss"));
    let mut diagnostics = Vec::new();
    if let Err(diagnostic) = &app_script_path {
        diagnostics.push(diagnostic.clone());
    }
    if let Err(diagnostic) = &app_style_path {
        diagnostics.push(diagnostic.clone());
    }
    let mut pages = Vec::with_capacity(routes.len());
    for (route, relative_base) in routes {
        let page_base = relative_base.join(&route);
        let script_relative = page_base.with_extension("js");
        let template_relative = page_base.with_extension("wxml");
        let style_relative = page_base.with_extension("wxss");
        let script_path = resolve_required_file(root, &script_relative, &script_relative);
        let template_path = resolve_required_file(root, &template_relative, &template_relative);
        let style_path = resolve_optional_file(root, &style_relative);
        if let Err(diagnostic) = &script_path {
            diagnostics.push(diagnostic.clone());
        }
        if let Err(diagnostic) = &template_path {
            diagnostics.push(diagnostic.clone());
        }
        if let Err(diagnostic) = &style_path {
            diagnostics.push(diagnostic.clone());
        }
        if let (Ok(script_path), Ok(template_path), Ok(style_path)) =
            (script_path, template_path, style_path)
        {
            pages.push(PageDefinition {
                route: relative_base
                    .join(route)
                    .to_string_lossy()
                    .replace('\\', "/"),
                script_path,
                template_path,
                style_path,
            });
        }
    }
    if diagnostics.is_empty() {
        Ok((
            app_script_path.map_err(|diagnostic| LoaderError::InvalidProject {
                diagnostics: vec![diagnostic],
            })?,
            app_style_path.map_err(|diagnostic| LoaderError::InvalidProject {
                diagnostics: vec![diagnostic],
            })?,
            pages,
        ))
    } else {
        Err(LoaderError::InvalidProject { diagnostics })
    }
}

fn add_route(
    route: &str,
    package_root: &Path,
    known_routes: &mut HashSet<String>,
    routes: &mut Vec<(PathBuf, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let route_path = Path::new(route);
    if route.is_empty() || validate_relative_path(route_path).is_err() {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::InvalidPagePath,
            path: PathBuf::from(route),
            message: "page route must be a normalized relative path without a file extension"
                .to_owned(),
            line: None,
            column: None,
        });
        return;
    }
    if route_path.extension().is_some() {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::InvalidPagePath,
            path: PathBuf::from(route),
            message: "page route must omit the source file extension".to_owned(),
            line: None,
            column: None,
        });
        return;
    }
    let full_route = package_root.join(route_path);
    let route_key = full_route
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if !known_routes.insert(route_key) {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::DuplicatePage,
            path: full_route,
            message: "page route is declared more than once".to_owned(),
            line: None,
            column: None,
        });
        return;
    }
    routes.push((route_path.to_path_buf(), package_root.to_path_buf()));
}

fn resolve_required_file(
    root: &Path,
    relative: &Path,
    diagnostic_path: &Path,
) -> Result<PathBuf, Diagnostic> {
    match resolve_file(root, relative) {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(Diagnostic {
            kind: DiagnosticKind::MissingFile,
            path: diagnostic_path.to_path_buf(),
            message: "required project file does not exist".to_owned(),
            line: None,
            column: None,
        }),
        Err(message) => Err(Diagnostic {
            kind: DiagnosticKind::UnsafePath,
            path: diagnostic_path.to_path_buf(),
            message,
            line: None,
            column: None,
        }),
    }
}

fn resolve_optional_file(root: &Path, relative: &Path) -> Result<Option<PathBuf>, Diagnostic> {
    resolve_file(root, relative).map_err(|message| Diagnostic {
        kind: DiagnosticKind::UnsafePath,
        path: relative.to_path_buf(),
        message,
        line: None,
        column: None,
    })
}

fn resolve_file(root: &Path, relative: &Path) -> Result<Option<PathBuf>, String> {
    validate_relative_path(relative)
        .map_err(|()| "resource path is not a safe relative path".to_owned())?;
    let candidate = root.join(relative);
    if !candidate.exists() {
        return Ok(None);
    }
    let canonical = fs::canonicalize(&candidate)
        .map_err(|error| format!("failed to resolve project resource: {error}"))?;
    if !canonical.starts_with(root) {
        return Err("resource resolves outside the project root".to_owned());
    }
    if !canonical.is_file() {
        return Err("resource path does not resolve to a file".to_owned());
    }
    Ok(Some(canonical))
}

fn validate_relative_path(path: &Path) -> Result<(), ()> {
    if path.as_os_str().is_empty() {
        return Err(());
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(());
        }
    }
    let value = path.to_string_lossy();
    if value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sdkwork-loader-{name}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_project(root: &Path) -> std::io::Result<()> {
        fs::create_dir_all(root.join("pages/home"))?;
        fs::write(root.join("app.json"), r#"{"pages":["pages/home/index"]}"#)?;
        fs::write(root.join("app.js"), "App({});")?;
        fs::write(root.join("pages/home/index.js"), "Page({});")?;
        fs::write(root.join("pages/home/index.wxml"), "<view>Home</view>")?;
        Ok(())
    }

    #[test]
    fn loads_project_and_resolves_page_files() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = temporary_directory("valid");
        write_project(&temporary)?;

        let project = load_project(&temporary)?;
        assert_eq!(project.pages.len(), 1);
        assert_eq!(project.pages[0].route, "pages/home/index");
        assert!(project.pages[0].style_path.is_none());
        assert_eq!(
            project.app_script_path.file_name(),
            Some(std::ffi::OsStr::new("app.js"))
        );

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn rejects_page_paths_with_parent_components() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = temporary_directory("unsafe");
        fs::create_dir_all(&temporary)?;
        fs::write(temporary.join("app.json"), r#"{"pages":["../outside"]}"#)?;
        fs::write(temporary.join("app.js"), "App({});")?;

        let Err(error) = load_project(&temporary) else {
            return Err("unsafe route unexpectedly loaded".into());
        };
        let diagnostics = match error {
            LoaderError::InvalidProject { diagnostics } => diagnostics,
            other => return Err(other.to_string().into()),
        };
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::InvalidPagePath)
        );

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn rejects_entry_page_not_registered_in_app_json() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = temporary_directory("entry-page");
        fs::create_dir_all(&temporary)?;
        fs::write(
            temporary.join("app.json"),
            r#"{"pages":["pages/home/index"],"entryPagePath":"pages/unknown/index"}"#,
        )?;

        let error = match load_project(&temporary) {
            Ok(_) => return Err("unknown entry page unexpectedly loaded".into()),
            Err(error) => error,
        };
        let diagnostics = match error {
            LoaderError::InvalidProject { diagnostics } => diagnostics,
            other => return Err(other.to_string().into()),
        };
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::InvalidManifest
                && diagnostic.message.contains("entryPagePath")
        }));

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn includes_json_line_and_column_for_invalid_manifest() -> Result<(), Box<dyn std::error::Error>>
    {
        let temporary = temporary_directory("json");
        fs::create_dir_all(&temporary)?;
        fs::write(temporary.join("app.json"), "{\n  invalid\n}")?;

        let Err(error) = load_project(&temporary) else {
            return Err("invalid JSON unexpectedly loaded".into());
        };
        let diagnostics = match error {
            LoaderError::InvalidProject { diagnostics } => diagnostics,
            other => return Err(other.to_string().into()),
        };
        assert_eq!(diagnostics[0].line, Some(2));
        assert!(diagnostics[0].column.is_some());

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[test]
    fn read_file_rejects_escaping_resource_paths() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = temporary_directory("read-path");
        write_project(&temporary)?;
        let project = load_project(&temporary)?;

        let error = project.read_file("../outside.txt");
        assert!(matches!(error, Err(LoaderError::InvalidProject { .. })));

        fs::remove_dir_all(temporary)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_project_resources_symlinked_outside_the_root()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let temporary = temporary_directory("symlink");
        let outside = temporary.with_extension("outside");
        write_project(&temporary)?;
        fs::write(&outside, "external")?;
        fs::remove_file(temporary.join("pages/home/index.wxml"))?;
        symlink(&outside, temporary.join("pages/home/index.wxml"))?;

        let error = load_project(&temporary);
        assert!(matches!(error, Err(LoaderError::InvalidProject { .. })));

        fs::remove_dir_all(temporary)?;
        fs::remove_file(outside)?;
        Ok(())
    }
}
