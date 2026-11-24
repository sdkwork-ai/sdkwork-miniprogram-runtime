//! Loopback-only HTTP host for the preview and source workbench.

use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_EDIT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 5_000;
const MAX_TREE_DEPTH: usize = 16;

#[derive(Clone)]
struct ActiveProject {
    root: PathBuf,
    search_root: PathBuf,
    name: String,
}

/// Starts the loopback workbench, optionally opening it in a browser window.
pub fn serve(root: &Path, project_name: &str, launch_window: bool) -> Result<(), Box<dyn Error>> {
    let root = fs::canonicalize(root)?;
    let project = Arc::new(RwLock::new(ActiveProject {
        name: project_name.to_owned(),
        search_root: root.parent().unwrap_or(&root).to_path_buf(),
        root: root.clone(),
    }));
    let server = Server::http(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(io::Error::other)?;
    let address = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| io::Error::other("workbench host did not bind an IP address"))?;
    let url = format!("http://{address}/");

    if launch_window {
        super::viewer::launch_url(&url)?;
    }

    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "{}",
        serde_json::to_string(&json!({
            "state": "Visible",
            "projectRoot": root,
            "workbenchUrl": url,
            "windowLaunched": launch_window,
            "defaultDevice": "phone",
            "editor": "Monaco Editor",
            "limitations": [
                "supported WXML, page-data and WXSS subsets are rendered",
                "wx APIs, events and full WeChat compatibility remain unsupported"
            ]
        }))?
    )?;
    stdout.flush()?;

    loop {
        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(request)) => handle_request(request, &project),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn handle_request(mut request: Request, project: &Arc<RwLock<ActiveProject>>) {
    let result = route_request(&mut request, project);
    let (status, content_type, body) = match result {
        Ok((content_type, body)) => (StatusCode(200), content_type, body),
        Err((status, message)) => (status, "text/plain; charset=utf-8", message),
    };
    if let Ok(header) = Header::from_bytes("Content-Type", content_type) {
        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(header);
        let _ = request.respond(response);
    }
}

fn route_request(
    request: &mut Request,
    active_project: &Arc<RwLock<ActiveProject>>,
) -> Result<(&'static str, String), (StatusCode, String)> {
    let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
    if request.method() == &Method::Put && path == "/api/project" {
        require_same_origin(request)?;
        return switch_project(request, active_project);
    }
    let current = active_project
        .read()
        .map_err(|_| (StatusCode(500), "project state is unavailable".to_owned()))?
        .clone();
    match (request.method(), path) {
        (&Method::Get, "/") => api_workbench(request, &current),
        (&Method::Get, "/api/tree") => api_tree(request, &current),
        (&Method::Get, "/api/pages") => api_pages(request, &current),
        (&Method::Get, "/api/projects") => api_project_choices(request, &current, query),
        (&Method::Get, "/api/project") => api_project_info(request, &current),
        (&Method::Get, "/api/inspect") => api_inspect(request, &current, query),
        (&Method::Get, "/api/file") => api_read_file(request, &current, query),
        (&Method::Put, "/api/file") => {
            require_same_origin(request)?;
            save_file(request, &current.root)
        }
        (&Method::Get, "/api/preview") => api_preview(request, &current, query),
        _ => Err((StatusCode(404), "not found".to_owned())),
    }
}

fn api_workbench(
    request: &Request,
    current: &ActiveProject,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    Ok((
        "text/html; charset=utf-8",
        super::workbench::generate_html(&current.name),
    ))
}

fn api_tree(
    request: &Request,
    current: &ActiveProject,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let mut count = 0;
    let entries = tree_entries(&current.root, &current.root, 0, &mut count)
        .map_err(|error| (StatusCode(500), error.to_string()))?;
    Ok((
        "application/json; charset=utf-8",
        json!({"rootName": current.name, "entries": entries}).to_string(),
    ))
}

fn api_pages(
    request: &Request,
    current: &ActiveProject,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let project = sdkwork_miniprogram_package_loader::load_project(&current.root)
        .map_err(|error| (StatusCode(422), format!("{error:?}")))?;
    let routes = project
        .pages
        .iter()
        .map(|page| page.route.as_str())
        .collect::<Vec<_>>();
    Ok((
        "application/json; charset=utf-8",
        json!({"pages": routes, "entryPage": project.manifest.entry_page_path}).to_string(),
    ))
}

fn api_project_choices(
    request: &Request,
    current: &ActiveProject,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let parent =
        query_value(query, "parent").map_or_else(|| current.search_root.clone(), PathBuf::from);
    let parent = fs::canonicalize(&parent).map_err(|error| {
        (
            StatusCode(422),
            format!("project search folder is unavailable: {error}"),
        )
    })?;
    if !parent.is_dir() {
        return Err((
            StatusCode(422),
            "project search path is not a folder".to_owned(),
        ));
    }
    let entries = fs::read_dir(parent).map_err(|error| (StatusCode(500), error.to_string()))?;
    let mut projects = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let candidate = fs::canonicalize(entry.path()).ok()?;
            if !candidate.join("app.json").is_file() {
                return None;
            }
            Some(json!({
                "name": candidate.file_name()?.to_string_lossy(),
                "path": candidate.to_string_lossy()
            }))
        })
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok((
        "application/json; charset=utf-8",
        json!({"projects": projects, "currentPath": current.root}).to_string(),
    ))
}

fn api_project_info(
    request: &Request,
    current: &ActiveProject,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    Ok((
        "application/json; charset=utf-8",
        json!({"rootName": current.name, "rootPath": current.root}).to_string(),
    ))
}

fn api_inspect(
    request: &Request,
    current: &ActiveProject,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let route =
        query_value(query, "page").ok_or((StatusCode(400), "missing page route".to_owned()))?;
    let preview = render_page(&current.root, Some(&route))
        .map_err(|error| (StatusCode(422), error.to_string()))?;
    let inspection =
        extract_page_inspection(&preview).map_err(|error| (StatusCode(500), error.to_string()))?;
    Ok(("application/json; charset=utf-8", inspection.to_string()))
}

fn api_read_file(
    request: &Request,
    current: &ActiveProject,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let relative =
        query_value(query, "path").ok_or((StatusCode(400), "missing file path".to_owned()))?;
    let path = safe_existing_file(&current.root, &relative)?;
    let metadata = fs::metadata(&path).map_err(|error| (StatusCode(404), error.to_string()))?;
    if metadata.len() > MAX_EDIT_BYTES {
        return Err((StatusCode(413), "file is larger than 4 MiB".to_owned()));
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| (StatusCode(415), format!("file is not UTF-8 text: {error}")))?;
    Ok((
        "application/json; charset=utf-8",
        json!({"path": relative, "content": content}).to_string(),
    ))
}

fn api_preview(
    request: &Request,
    current: &ActiveProject,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let page = query_value(query, "page");
    render_page(&current.root, page.as_deref())
        .map(|html| ("text/html; charset=utf-8", html))
        .map_err(|error| (StatusCode(422), error.to_string()))
}
fn switch_project(
    request: &mut Request,
    active_project: &Arc<RwLock<ActiveProject>>,
) -> Result<(&'static str, String), (StatusCode, String)> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(16 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|error| (StatusCode(400), error.to_string()))?;
    if body.len() > 16 * 1024 {
        return Err((
            StatusCode(413),
            "project path request is too large".to_owned(),
        ));
    }
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|error| (StatusCode(400), format!("invalid JSON body: {error}")))?;
    let requested_path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or((StatusCode(400), "missing project path".to_owned()))?;
    let canonical_root = fs::canonicalize(requested_path).map_err(|error| {
        (
            StatusCode(422),
            format!("project path is unavailable: {error}"),
        )
    })?;
    let project =
        sdkwork_miniprogram_package_loader::load_project(&canonical_root).map_err(|error| {
            (
                StatusCode(422),
                format!("project validation failed: {error:?}"),
            )
        })?;
    let name = project
        .root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Mini Program")
        .to_owned();
    let project_root = project.root;
    let active_project_name = name;
    let search_root = project_root.parent().unwrap_or(&project_root).to_path_buf();
    *active_project
        .write()
        .map_err(|_| (StatusCode(500), "project state is unavailable".to_owned()))? =
        ActiveProject {
            root: project_root.clone(),
            search_root,
            name: active_project_name.clone(),
        };
    Ok((
        "application/json; charset=utf-8",
        json!({"rootName": active_project_name, "rootPath": project_root}).to_string(),
    ))
}

fn require_same_origin(request: &Request) -> Result<(), (StatusCode, String)> {
    if is_loopback_request(request) && is_loopback_origin(request) {
        Ok(())
    } else {
        Err((StatusCode(403), "same-origin requests only".to_owned()))
    }
}

fn require_loopback_header(request: &Request) -> Result<(), (StatusCode, String)> {
    if is_loopback_request(request) {
        Ok(())
    } else {
        Err((StatusCode(403), "loopback requests only".to_owned()))
    }
}

fn is_loopback_origin(request: &Request) -> bool {
    let host = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Host"))
        .map(|header| header.value.as_str());
    let origin = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str());
    host.zip(origin)
        .is_some_and(|(host, origin)| origin.eq_ignore_ascii_case(&format!("http://{host}")))
}

fn is_loopback_request(request: &Request) -> bool {
    let Some(host) = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Host"))
        .map(|header| header.value.as_str())
    else {
        return false;
    };
    host.starts_with("127.0.0.1:") || host.starts_with("localhost:")
}

fn tree_entries(
    root: &Path,
    directory: &Path,
    depth: usize,
    count: &mut usize,
) -> io::Result<Vec<Value>> {
    if depth > MAX_TREE_DEPTH || *count >= MAX_TREE_ENTRIES {
        return Ok(Vec::new());
    }
    let mut children = Vec::new();
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if *count >= MAX_TREE_ENTRIES {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), ".git" | "node_modules" | "target") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let canonical = fs::canonicalize(entry.path())?;
        if !canonical.starts_with(root) {
            continue;
        }
        let relative = canonical
            .strip_prefix(root)
            .map_err(io::Error::other)?
            .to_string_lossy()
            .replace('\\', "/");
        *count += 1;
        if metadata.is_dir() {
            children.push(json!({
                "kind": "directory",
                "name": name,
                "path": relative,
                "children": tree_entries(root, &canonical, depth + 1, count)?
            }));
        } else if metadata.is_file() {
            children.push(json!({"kind": "file", "name": name, "path": relative}));
        }
    }
    Ok(children)
}

fn safe_existing_file(root: &Path, relative: &str) -> Result<PathBuf, (StatusCode, String)> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err((StatusCode(400), "invalid relative file path".to_owned()));
    }
    let unresolved = root.join(relative_path);
    let path =
        fs::canonicalize(&unresolved).map_err(|error| (StatusCode(404), error.to_string()))?;
    if !path.starts_with(root) {
        return Err((
            StatusCode(403),
            "file resolves outside the project".to_owned(),
        ));
    }
    if !path.is_file() {
        return Err((StatusCode(400), "path is not a regular file".to_owned()));
    }
    Ok(path)
}

fn save_file(
    request: &mut Request,
    root: &Path,
) -> Result<(&'static str, String), (StatusCode, String)> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_EDIT_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| (StatusCode(400), error.to_string()))?;
    if body.len() as u64 > MAX_EDIT_BYTES {
        return Err((StatusCode(413), "file is larger than 4 MiB".to_owned()));
    }
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|error| (StatusCode(400), format!("invalid JSON body: {error}")))?;
    let relative = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or((StatusCode(400), "missing file path".to_owned()))?;
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .ok_or((StatusCode(400), "missing file content".to_owned()))?;
    let path = safe_existing_file(root, relative)?;
    fs::write(&path, content).map_err(|error| (StatusCode(500), error.to_string()))?;
    Ok((
        "application/json; charset=utf-8",
        json!({"saved": true}).to_string(),
    ))
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (percent_decode(name).ok()? == key)
            .then(|| percent_decode(value).ok())
            .flatten()
    })
}

fn percent_decode(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]).ok_or(())?;
                let low = hex_value(bytes[index + 2]).ok_or(())?;
                decoded.push((high << 4) | low);
                index += 2;
            }
            b'%' => return Err(()),
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8(decoded).map_err(|_| ())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn render_page(root: &Path, requested_page: Option<&str>) -> Result<String, Box<dyn Error>> {
    use sdkwork_miniprogram_execution_engine::{ApplicationManifest, RuntimeBuilder};
    use sdkwork_miniprogram_package_loader::load_project;

    let project = load_project(root)?;
    let pages = project
        .pages
        .iter()
        .map(|page| page.route.clone())
        .collect::<Vec<_>>();
    let entry_page = requested_page
        .filter(|route| pages.iter().any(|page| page == route))
        .map_or_else(
            || {
                project
                    .manifest
                    .entry_page_path
                    .clone()
                    .unwrap_or_else(|| pages[0].clone())
            },
            str::to_owned,
        );
    let app_id = project
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("preview");
    let application = RuntimeBuilder::new()
        .build()?
        .load_application(ApplicationManifest {
            app_id: app_id.to_owned(),
            version: "0.0.0-preview".to_owned(),
            entry_page: entry_page.clone(),
            pages,
        })?;
    let app_script = String::from_utf8(
        project.read_file(project.app_script_path.strip_prefix(&project.root)?)?,
    )?;
    application.register_javascript_source("app.js", &app_script)?;
    register_pages(&application, &project)?;
    application.launch()?;
    let _ = application.invoke_app_lifecycle("onLaunch")?;
    application.show()?;
    let _ = application.invoke_app_lifecycle("onShow")?;
    let (page_index, page) = project
        .pages
        .iter()
        .enumerate()
        .find(|(_, page)| page.route == entry_page)
        .ok_or("entry page was not found")?;
    let _ = application.invoke_page_lifecycle(page_index, "onLoad")?;
    let _ = application.invoke_page_lifecycle(page_index, "onShow")?;
    let rendered = render_project_page(&project, page, &application, page_index)?;
    Ok(super::viewer::generate_page_html(
        project_name_for(&project.root),
        &rendered,
    ))
}

fn register_pages(
    application: &sdkwork_miniprogram_execution_engine::ApplicationHandle,
    project: &sdkwork_miniprogram_package_loader::MiniProgramProject,
) -> Result<(), Box<dyn Error>> {
    for page in &project.pages {
        let script =
            String::from_utf8(project.read_file(page.script_path.strip_prefix(&project.root)?)?)?;
        application.register_javascript_source(&page.route, &script)?;
    }
    Ok(())
}

fn render_project_page(
    project: &sdkwork_miniprogram_package_loader::MiniProgramProject,
    page: &sdkwork_miniprogram_package_loader::PageDefinition,
    application: &sdkwork_miniprogram_execution_engine::ApplicationHandle,
    page_index: usize,
) -> Result<super::viewer::ViewerPage, Box<dyn Error>> {
    let source =
        String::from_utf8(project.read_file(page.template_path.strip_prefix(&project.root)?)?)?;
    let tree = sdkwork_miniprogram_wxml_compiler::parse(
        page.template_path.display().to_string(),
        &source,
    )?;
    let styles = read_page_styles(project, page)?;
    let style_tree = sdkwork_miniprogram_headless_renderer::build_with_styles(
        &tree,
        page.style_path
            .as_ref()
            .map_or("<missing>", |path| path.to_str().unwrap_or("<page-style>")),
        &styles,
    );
    let page_data =
        application.javascript_registration_snapshot()?["pages"][page_index]["data"].clone();
    let rendered = sdkwork_miniprogram_headless_renderer::build_with_data(
        &tree,
        page.template_path.display().to_string(),
        &page_data,
        1_000,
    );
    Ok(super::viewer::ViewerPage {
        route: page.route.clone(),
        render_tree: serde_json::to_value(rendered.tree)?,
        computed_styles: serde_json::to_value(style_tree.computed_styles)?,
        style_diagnostics: serde_json::to_value(style_tree.diagnostics)?,
        page_data,
    })
}

fn read_page_styles(
    project: &sdkwork_miniprogram_package_loader::MiniProgramProject,
    page: &sdkwork_miniprogram_package_loader::PageDefinition,
) -> Result<String, Box<dyn Error>> {
    let mut styles = project
        .app_style_path
        .as_ref()
        .map(|path| {
            let relative = path.strip_prefix(&project.root).map_err(io::Error::other)?;
            String::from_utf8(project.read_file(relative).map_err(io::Error::other)?)
                .map_err(io::Error::other)
        })
        .transpose()?
        .unwrap_or_default();
    if let Some(style_path) = &page.style_path {
        let relative = style_path
            .strip_prefix(&project.root)
            .map_err(io::Error::other)?;
        let page_styles = String::from_utf8(project.read_file(relative)?)?;
        if !styles.is_empty() && !page_styles.is_empty() {
            styles.push('\n');
        }
        styles.push_str(&page_styles);
    }
    Ok(styles)
}

fn project_name_for(root: &Path) -> &str {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Mini Program")
}

fn extract_page_inspection(html: &str) -> Result<Value, Box<dyn Error>> {
    let body_start = html
        .find("<script id=\"inspector-data\"")
        .ok_or("missing inspector payload")?;
    let start = html[body_start..]
        .find('>')
        .map(|position| body_start + position + 1)
        .ok_or("invalid inspector payload")?;
    let end = html[start..]
        .find("</script>")
        .ok_or("invalid inspector payload")?
        + start;
    Ok(serde_json::from_str(&html[start..end])?)
}
