//! Loopback-only HTTP host for the preview and source workbench.

use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use tungstenite::Message;

const MAX_EDIT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 5_000;
const MAX_TREE_DEPTH: usize = 16;

#[derive(Clone)]
struct ActiveProject {
    root: PathBuf,
    search_root: PathBuf,
    name: String,
    app_id: String,
    app: Arc<RwLock<sdkwork_miniprogram_execution_engine::ApplicationHandle>>,
}

struct DebugTargets {
    target_id: String,
    ws_path: String,
    ws_url: String,
    title: String,
    app: Arc<RwLock<sdkwork_miniprogram_execution_engine::ApplicationHandle>>,
    active_session: std::sync::Mutex<bool>,
}

/// Starts the loopback workbench, optionally opening it in a browser window.
pub fn serve(root: &Path, project_name: &str, launch_window: bool) -> Result<(), Box<dyn Error>> {
    let root = fs::canonicalize(root)?;
    let app_id = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mini-program")
        .to_owned();
    let project = Arc::new(RwLock::new(create_runtime_application(
        &root,
        &app_id,
        project_name,
    )?));
    let server = Server::http(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(io::Error::other)?;
    let address = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| io::Error::other("workbench host did not bind an IP address"))?;
    let url = format!("http://{address}/");
    let active = project
        .read()
        .map_err(|_| io::Error::other("project state is unavailable"))?
        .clone();
    let target_id = format!("mini-program:{}", active.app_id);
    let ws_path = format!("/devtools/page/{}", active.app_id);
    let ws_url = format!("ws://{address}{ws_path}");
    let targets = Arc::new(DebugTargets {
        target_id: target_id.clone(),
        ws_path,
        ws_url: ws_url.clone(),
        title: project_name.to_owned(),
        app: active.app.clone(),
        active_session: std::sync::Mutex::new(false),
    });

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
            "cdpTarget": {
                "targetId": target_id,
                "webSocketDebuggerUrl": targets.ws_url,
                "discoveryUrl": format!("http://{address}/json/list")
            },
            "limitations": [
                "supported WXML, page-data and WXSS subsets are rendered",
                "wx APIs, events and full WeChat compatibility remain unsupported"
            ]
        }))?
    )?;
    stdout.flush()?;

    loop {
        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(request)) => handle_request(request, &project, &targets),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn handle_request(
    mut request: Request,
    project: &Arc<RwLock<ActiveProject>>,
    targets: &Arc<DebugTargets>,
) {
    if request.url().split('?').next() == Some(targets.ws_path.as_str()) {
        upgrade_debug_websocket(request, targets);
        return;
    }
    let result = route_request(&mut request, project, targets);
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
    targets: &Arc<DebugTargets>,
) -> Result<(&'static str, String), (StatusCode, String)> {
    let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
    if request.method() == &Method::Put && path == "/api/project" {
        require_same_origin(request)?;
        return switch_project(request, active_project, targets);
    }
    if request.method() == &Method::Get && matches!(path, "/json" | "/json/list") {
        return target_list(request, targets);
    }
    if request.method() == &Method::Get && path == "/json/version" {
        require_loopback_header(request)?;
        return Ok((
            "application/json; charset=utf-8",
            json!({
                "Browser": "SDKWORK Mini Program Runtime/0.1",
                "Protocol-Version": "1.3",
                "webSocketDebuggerUrl": targets.ws_url,
            })
            .to_string(),
        ));
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
        (&Method::Get, "/api/inspect") => api_inspect(request, &current, targets, query),
        (&Method::Get, "/api/file") => api_read_file(request, &current, query),
        (&Method::Put, "/api/file") => {
            require_same_origin(request)?;
            save_file(request, active_project, targets)
        }
        (&Method::Get, "/api/preview") => api_preview(request, &current, targets, query),
        _ => Err((StatusCode(404), "not found".to_owned())),
    }
}

fn target_list(
    request: &Request,
    targets: &DebugTargets,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let target = json!({
        "description": "SDKWORK Mini Program Runtime application target",
        "devtoolsFrontendUrl": format!("/devtools/inspector.html?ws={}", targets.ws_url.trim_start_matches("ws://")),
        "id": targets.target_id,
        "title": targets.title,
        "type": "node",
        "url": format!("miniprogram://{}", targets.title),
        "webSocketDebuggerUrl": targets.ws_url,
    });
    Ok((
        "application/json; charset=utf-8",
        json!([target]).to_string(),
    ))
}

fn upgrade_debug_websocket(request: Request, targets: &Arc<DebugTargets>) {
    let key = match validate_websocket_request(&request) {
        Ok(key) => key,
        Err((status, message)) => {
            let _ = request.respond(Response::from_string(message).with_status_code(status));
            return;
        }
    };
    if let Err(status) = acquire_debug_session(targets) {
        let message = if status == StatusCode(409) {
            "debug target already has an active session"
        } else {
            "debug target unavailable"
        };
        let _ = request.respond(Response::from_string(message).with_status_code(status));
        return;
    }
    let response = match websocket_upgrade_response(&key) {
        Ok(response) => response,
        Err(message) => {
            release_debug_session(targets);
            let _ =
                request.respond(Response::from_string(message).with_status_code(StatusCode(400)));
            return;
        }
    };
    let stream = request.upgrade("websocket", response);
    let failure_targets = Arc::clone(targets);
    let targets = Arc::clone(targets);
    let spawn = thread::Builder::new()
        .name("sdkwork-miniprogram-cdp-session".to_owned())
        .spawn(move || {
            run_debug_websocket(stream, &targets);
        });
    if spawn.is_err() {
        release_debug_session(&failure_targets);
    }
}

fn validate_websocket_request(request: &Request) -> Result<String, (StatusCode, String)> {
    if !is_loopback_request(request) {
        return Err((
            StatusCode(403),
            "loopback WebSocket clients only".to_owned(),
        ));
    }
    let is_upgrade = request.method() == &Method::Get
        && request.headers().iter().any(|header| {
            header.field.equiv("Upgrade") && header.value.as_str().eq_ignore_ascii_case("websocket")
        })
        && request.headers().iter().any(|header| {
            header.field.equiv("Connection")
                && header
                    .value
                    .as_str()
                    .to_ascii_lowercase()
                    .contains("upgrade")
        });
    if !is_upgrade {
        return Err((StatusCode(400), "WebSocket upgrade required".to_owned()));
    }
    let origin = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str());
    let host = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Host"))
        .map(|header| header.value.as_str());
    if origin
        .zip(host)
        .is_some_and(|(origin, host)| origin != format!("http://{host}"))
    {
        return Err((StatusCode(403), "WebSocket Origin rejected".to_owned()));
    }
    if !request
        .headers()
        .iter()
        .any(|header| header.field.equiv("Sec-WebSocket-Version") && header.value.as_str() == "13")
    {
        return Err((StatusCode(400), "WebSocket version 13 required".to_owned()));
    }
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Sec-WebSocket-Key"))
        .map(|header| header.value.as_str().to_owned())
        .ok_or((StatusCode(400), "missing Sec-WebSocket-Key".to_owned()))
}

fn websocket_upgrade_response(key: &str) -> Result<Response<io::Empty>, String> {
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let headers = [
        Header::from_str("Upgrade: websocket"),
        Header::from_str("Connection: Upgrade"),
        Header::from_str(&format!("Sec-WebSocket-Accept: {accept}")),
    ];
    let mut response = Response::new_empty(StatusCode(101));
    for header in headers {
        response = response.with_header(header.map_err(|()| "invalid WebSocket response header")?);
    }
    Ok(response)
}

fn acquire_debug_session(targets: &DebugTargets) -> Result<(), StatusCode> {
    let mut active = targets.active_session.lock().map_err(|_| StatusCode(503))?;
    if *active {
        return Err(StatusCode(409));
    }
    *active = true;
    Ok(())
}

fn release_debug_session(targets: &DebugTargets) {
    if let Ok(mut active) = targets.active_session.lock() {
        *active = false;
    }
}

fn run_debug_websocket<S>(stream: S, targets: &Arc<DebugTargets>)
where
    S: Read + Write,
{
    let mut socket =
        tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    loop {
        match socket.read() {
            Ok(Message::Text(message)) => {
                let responses = targets
                    .app
                    .read()
                    .map_err(|_| "Runtime app is unavailable".to_owned())
                    .and_then(|app| {
                        app.dispatch_inspector_message(message.as_str())
                            .map_err(|error| error.to_string())
                    });
                match responses {
                    Ok(responses) => {
                        for response in responses {
                            if socket.send(Message::Text(response.into())).is_err() {
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "CDP protocol message dispatch failed");
                        break;
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                if socket.send(Message::Pong(payload)).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Binary(_) | Message::Pong(_) | Message::Frame(_)) => {}
        }
    }
    let _ = socket.close(None);
    release_debug_session(targets);
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
    targets: &DebugTargets,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let route =
        query_value(query, "page").ok_or((StatusCode(400), "missing page route".to_owned()))?;
    let project = sdkwork_miniprogram_package_loader::load_project(&current.root)
        .map_err(|error| (StatusCode(422), format!("{error:?}")))?;
    let (page_index, page) = project
        .pages
        .iter()
        .enumerate()
        .find(|(_, page)| page.route == route)
        .ok_or((StatusCode(404), "page route not found".to_owned()))?;
    let app = targets.app.read().map_err(|_| {
        (
            StatusCode(500),
            "Runtime application is unavailable".to_owned(),
        )
    })?;
    let inspection = render_project_page(&project, page, &app, page_index)
        .map_err(|error| (StatusCode(422), error.to_string()))?;
    let payload = json!({
        "route": inspection.route,
        "pageData": inspection.page_data,
        "console": inspection.console_events,
        "network": inspection.network_events,
        "target": inspection.target,
        "protocolVersion": "sdkwork-miniprogram-inspector/0.1",
        "debugCapabilities": inspection.debug_capabilities,
        "renderTree": inspection.render_tree,
        "computedStyles": inspection.computed_styles,
        "diagnostics": inspection.style_diagnostics,
    });
    Ok(("application/json; charset=utf-8", payload.to_string()))
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
    targets: &DebugTargets,
    query: &str,
) -> Result<(&'static str, String), (StatusCode, String)> {
    require_loopback_header(request)?;
    let page = query_value(query, "page");
    let project = sdkwork_miniprogram_package_loader::load_project(&current.root)
        .map_err(|error| (StatusCode(422), format!("{error:?}")))?;
    let route = page
        .or_else(|| project.manifest.entry_page_path.clone())
        .or_else(|| project.pages.first().map(|page| page.route.clone()))
        .ok_or((StatusCode(422), "project has no entry page".to_owned()))?;
    let (page_index, page) = project
        .pages
        .iter()
        .enumerate()
        .find(|(_, page)| page.route == route)
        .ok_or((StatusCode(404), "page route not found".to_owned()))?;
    let app = targets.app.read().map_err(|_| {
        (
            StatusCode(500),
            "Runtime application is unavailable".to_owned(),
        )
    })?;
    let rendered = render_project_page(&project, page, &app, page_index)
        .map_err(|error| (StatusCode(422), error.to_string()))?;
    Ok((
        "text/html; charset=utf-8",
        super::viewer::generate_page_html(&current.name, &rendered),
    ))
}
fn switch_project(
    request: &mut Request,
    active_project: &Arc<RwLock<ActiveProject>>,
    targets: &Arc<DebugTargets>,
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
    let replacement_id = format!(
        "mini-program:{}",
        canonical_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let replacement = create_runtime_application(
        &canonical_root,
        &replacement_id,
        canonical_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    )
    .map_err(|error| {
        (
            StatusCode(422),
            format!("project validation failed: {error}"),
        )
    })?;
    let active_project_name = replacement.name.clone();
    let project_root = replacement.root.clone();
    let app = replacement.app.clone();
    *active_project
        .write()
        .map_err(|_| (StatusCode(500), "project state is unavailable".to_owned()))? = replacement;
    let replacement_app = app
        .read()
        .map_err(|_| {
            (
                StatusCode(500),
                "Runtime application is unavailable".to_owned(),
            )
        })?
        .clone();
    *targets
        .app
        .write()
        .map_err(|_| (StatusCode(500), "debug target is unavailable".to_owned()))? =
        replacement_app;
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

fn create_runtime_application(
    root: &Path,
    app_id: &str,
    display_name: &str,
) -> Result<ActiveProject, Box<dyn Error>> {
    use sdkwork_miniprogram_execution_engine::{ApplicationManifest, RuntimeBuilder};
    use sdkwork_miniprogram_package_loader::load_project;

    let project = load_project(root)?;
    let pages = project
        .pages
        .iter()
        .map(|page| page.route.clone())
        .collect::<Vec<_>>();
    let entry_page = project
        .manifest
        .entry_page_path
        .clone()
        .unwrap_or_else(|| pages[0].clone());
    let runtime = RuntimeBuilder::new().build()?;
    let application = runtime.load_application(ApplicationManifest {
        app_id: app_id.to_owned(),
        version: "0.0.0-preview".to_owned(),
        entry_page,
        pages,
    })?;
    let app_source = String::from_utf8(
        project.read_file(project.app_script_path.strip_prefix(&project.root)?)?,
    )?;
    application.register_javascript_source("app.js", &app_source)?;
    register_pages(&application, &project)?;
    application.launch()?;
    let _ = application.invoke_app_lifecycle("onLaunch")?;
    application.show()?;
    let _ = application.invoke_app_lifecycle("onShow")?;
    let entry_index = project
        .pages
        .iter()
        .position(|page| page.route == application.manifest().entry_page)
        .ok_or("entry page is missing from loaded project")?;
    let _ = application.invoke_page_lifecycle(entry_index, "onLoad")?;
    let _ = application.invoke_page_lifecycle(entry_index, "onShow")?;
    Ok(ActiveProject {
        root: project.root.clone(),
        search_root: project.root.parent().unwrap_or(&project.root).to_path_buf(),
        name: display_name.to_owned(),
        app_id: app_id.to_owned(),
        app: Arc::new(RwLock::new(application)),
    })
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
    active_project: &Arc<RwLock<ActiveProject>>,
    targets: &Arc<DebugTargets>,
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
    let current = active_project
        .read()
        .map_err(|_| (StatusCode(500), "project state is unavailable".to_owned()))?
        .clone();
    let path = safe_existing_file(&current.root, relative)?;
    fs::write(&path, content).map_err(|error| (StatusCode(500), error.to_string()))?;
    if matches!(
        Path::new(relative)
            .extension()
            .and_then(|value| value.to_str()),
        Some("js" | "ts")
    ) {
        let replacement = create_runtime_application(&current.root, &current.app_id, &current.name)
            .map_err(|error| {
                (
                    StatusCode(422),
                    format!("saved file but Runtime reload failed: {error}"),
                )
            })?;
        let replacement_app = replacement
            .app
            .read()
            .map_err(|_| {
                (
                    StatusCode(500),
                    "Runtime application is unavailable".to_owned(),
                )
            })?
            .clone();
        *targets
            .app
            .write()
            .map_err(|_| (StatusCode(500), "debug target is unavailable".to_owned()))? =
            replacement_app;
        *active_project
            .write()
            .map_err(|_| (StatusCode(500), "project state is unavailable".to_owned()))? =
            replacement;
    }
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
    let debug = application.javascript_debug_snapshot()?;
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
        console_events: debug.get("console").cloned().unwrap_or_else(|| json!([])),
        network_events: debug.get("network").cloned().unwrap_or_else(|| json!([])),
        target: debug.get("target").cloned().unwrap_or_else(|| json!({})),
        debug_capabilities: debug
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| json!({})),
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
