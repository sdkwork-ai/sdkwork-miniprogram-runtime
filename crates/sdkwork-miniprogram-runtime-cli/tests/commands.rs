//! CLI behavior tests through the compiled command entrypoint.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tungstenite::{Message, connect};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sdkwork-miniprogram-runtime-cli"))
}

fn example_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mini-program")
}

#[test]
fn inspects_example_project_and_reports_partial_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let output = cli().arg("inspect").arg(example_project()).output()?;

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(document["state"], "Visible");
    assert_eq!(document["pages"][0]["route"], "pages/index/index");
    assert_eq!(document["javascriptRegistrations"]["appSource"], "app.js");
    assert_eq!(
        document["javascriptRegistrations"]["pages"][0]["data"]["title"],
        "Hello, Mini Program Runtime"
    );
    assert_eq!(document["pages"][0]["lifecycle"]["onLoadResult"], "true");
    assert_eq!(
        document["javascriptRegistrations"]["pages"][0]["data"]["loaded"],
        true
    );
    assert_eq!(document["capabilities"]["wxmlStructuralParsing"], "partial");
    assert_eq!(document["capabilities"]["wxssStructuralSubset"], "partial");
    assert_eq!(
        document["capabilities"]["stylesLayoutAndPixels"],
        "unsupported"
    );
    assert!(
        document["pages"][0]["renderNodeCount"]
            .as_u64()
            .unwrap_or(0)
            >= 5
    );
    assert_eq!(
        document["pages"][0]["computedStyles"]["0"]["padding"],
        "24rpx"
    );
    assert_eq!(
        document["pages"][0]["dataRenderDiagnostics"],
        serde_json::json!([])
    );
    assert_eq!(
        document["pages"][0]["dataRenderTree"]["children"][0]["children"][1]["children"][0]["value"],
        "Hello, Mini Program Runtime"
    );
    assert_eq!(
        document["pages"][0]["dataRenderTree"]["children"][0]["children"][3]["children"][0]["value"],
        "Ready"
    );
    assert_eq!(document["pages"][0]["pageData"]["loaded"], true);
    Ok(())
}

#[test]
fn reports_usage_for_missing_project_path() -> Result<(), Box<dyn std::error::Error>> {
    let output = cli().arg("inspect").output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("expected a project directory path"));
    Ok(())
}

#[test]
fn run_command_hosts_an_editable_preview_workbench() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = temporary_project()?;
    let second_fixture = create_second_project(&fixture)?;
    let mut workbench = start_workbench(&fixture)?;
    let result = verify_workbench(&fixture, &second_fixture, &workbench.address);
    let _ = workbench.child.kill();
    let _ = workbench.child.wait();
    let _ = std::fs::remove_dir_all(&fixture);
    let _ = std::fs::remove_dir_all(&second_fixture);
    result
}

fn temporary_project() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let fixture = std::env::temp_dir().join(format!(
        "sdkwork-mini-workbench-test-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir_all(fixture.join("pages/index"))?;
    for relative in [
        "app.json",
        "app.js",
        "app.wxss",
        "pages/index/index.js",
        "pages/index/index.json",
        "pages/index/index.wxml",
        "pages/index/index.wxss",
    ] {
        std::fs::copy(example_project().join(relative), fixture.join(relative))?;
    }
    Ok(fixture)
}

fn create_second_project(first: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let name = first
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("invalid fixture name")?;
    let sibling = first.with_file_name(format!("{name}-second"));
    std::fs::create_dir_all(sibling.join("pages/index"))?;
    for relative in [
        "app.json",
        "app.js",
        "app.wxss",
        "pages/index/index.js",
        "pages/index/index.json",
        "pages/index/index.wxml",
        "pages/index/index.wxss",
    ] {
        std::fs::copy(first.join(relative), sibling.join(relative))?;
    }
    let script_path = sibling.join("pages/index/index.js");
    let script = std::fs::read_to_string(&script_path)?
        .replace("Hello, Mini Program Runtime", "Second Mini Program");
    std::fs::write(script_path, script)?;
    Ok(sibling)
}

struct TestWorkbench {
    child: std::process::Child,
    address: String,
}

fn start_workbench(fixture: &std::path::Path) -> Result<TestWorkbench, Box<dyn std::error::Error>> {
    let mut child = cli()
        .arg("run")
        .arg(fixture)
        .arg("--no-launch")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("missing child stdout")?);
    let mut line = String::new();
    stdout.read_line(&mut line)?;
    let startup: serde_json::Value = serde_json::from_str(&line)?;
    assert_eq!(startup["state"], "Visible");
    assert_eq!(startup["defaultDevice"], "phone");
    assert_eq!(startup["windowLaunched"], false);
    let address = startup["workbenchUrl"]
        .as_str()
        .ok_or("missing workbench URL")?
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    Ok(TestWorkbench { child, address })
}

fn verify_workbench(
    fixture: &std::path::Path,
    second_fixture: &std::path::Path,
    address: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    verify_workbench_shell(address)?;
    verify_runtime_cdp_target(address)?;
    verify_inspector_and_projects(address, fixture)?;
    verify_saved_preview(address, fixture)?;
    verify_security_and_switching(address, second_fixture)?;
    Ok(())
}

fn verify_runtime_cdp_target(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (version_status, version) = http_request(address, "GET", "/json/version", "")?;
    assert_eq!(version_status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&version)?["Protocol-Version"],
        "1.3"
    );
    let (_, targets) = http_request(address, "GET", "/json/list", "")?;
    let targets: serde_json::Value = serde_json::from_str(&targets)?;
    let target = targets
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or("Runtime CDP target was not discovered")?;
    assert_eq!(target["type"], "node");
    assert!(target["id"].as_str().is_some());
    let websocket_url = target["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("Runtime target is missing its WebSocket URL")?;
    let (mut socket, response) = connect(websocket_url)?;
    assert_eq!(response.status().as_u16(), 101);
    assert!(
        connect(websocket_url).is_err(),
        "target accepts a single active CDP session"
    );
    socket.send(Message::Text(
        r#"{"id":1,"method":"Runtime.enable"}"#.into(),
    ))?;
    let response = read_cdp_response(&mut socket, 1)?;
    assert!(response.get("result").is_some());
    socket.send(Message::Text(
        r#"{"id":2,"method":"Runtime.evaluate","params":{"expression":"6 * 7","returnByValue":true,"contextId":1}}"#.into(),
    ))?;
    let response = read_cdp_response(&mut socket, 2)?;
    assert_eq!(response["result"]["result"]["value"], 42);
    socket.close(None)?;
    Ok(())
}

fn read_cdp_response<S: Read + Write>(
    socket: &mut tungstenite::WebSocket<S>,
    id: u64,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    loop {
        let message = socket.read()?;
        if message.is_text() {
            let response: serde_json::Value = serde_json::from_str(message.to_text()?)?;
            if response["id"].as_u64() == Some(id) {
                return Ok(response);
            }
        }
    }
}

fn verify_workbench_shell(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (status, html) = http_request(address, "GET", "/", "")?;
    assert_eq!(status, 200);
    for expected in [
        "Mini Program Workbench",
        "data-device=\"phone\"",
        "data-device=\"tablet\"",
        "data-device=\"pc\"",
        "Monaco Editor",
        "id=\"inspector-panel\"",
        "id=\"toggle-inspector\"",
        "id=\"close-inspector\"",
        "setInspectorVisible(!state.inspectorVisible)",
        "event.data.type === 'inspector-hide'",
        "event.source !== $('preview-frame').contentWindow",
        "data-inspector-tab=\"tree\"",
        "data-inspector-tab=\"console\"",
        "data-inspector-tab=\"network\"",
        "data-inspector-tab=\"target\"",
        "data-inspector-tab=\"diagnostics\"",
        "protocolVersion: data.protocolVersion",
        "runtime-console.json",
        "Filter console level",
        "wx.request provider",
        "id=\"switch-project\"",
        "id=\"project-path\"",
        "id=\"preview-frame\"",
    ] {
        assert!(
            html.contains(expected),
            "missing workbench element {expected}"
        );
    }
    Ok(())
}

fn verify_inspector_and_projects(
    address: &str,
    fixture: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (status, tree) = http_request(address, "GET", "/api/tree", "")?;
    assert_eq!(status, 200);
    assert!(
        serde_json::from_str::<serde_json::Value>(&tree)?["entries"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    let (status, inspection) = http_request(
        address,
        "GET",
        "/api/inspect?page=pages%2Findex%2Findex",
        "",
    )?;
    assert_eq!(status, 200);
    let inspection: serde_json::Value = serde_json::from_str(&inspection)?;
    assert_eq!(inspection["route"], "pages/index/index");
    assert_eq!(
        inspection["pageData"]["title"],
        "Hello, Mini Program Runtime"
    );
    assert!(
        inspection["renderTree"]["children"][0]["source"]["line"]
            .as_u64()
            .is_some()
    );
    assert!(inspection["computedStyles"].is_object());
    assert_eq!(
        inspection["protocolVersion"],
        "sdkwork-miniprogram-inspector/0.1"
    );
    assert_eq!(inspection["target"]["type"], "mini-program");
    assert_eq!(
        inspection["debugCapabilities"]["console"],
        "runtime-shim-recording-partial"
    );
    assert_eq!(
        inspection["debugCapabilities"]["network"],
        "requires-request-provider"
    );
    assert_eq!(inspection["console"][0]["level"], "info");
    assert_eq!(
        inspection["console"][0]["args"][0],
        "hello mini program runtime"
    );
    assert!(inspection["network"].as_array().is_some_and(Vec::is_empty));
    let (status, project) = http_request(address, "GET", "/api/project", "")?;
    assert_eq!(status, 200);
    let project: serde_json::Value = serde_json::from_str(&project)?;
    let canonical = std::fs::canonicalize(fixture)?;
    assert_eq!(project["rootPath"], canonical.to_string_lossy().as_ref());
    let (status, choices) = http_request(address, "GET", "/api/projects", "")?;
    assert_eq!(status, 200);
    assert!(
        serde_json::from_str::<serde_json::Value>(&choices)?["projects"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    Ok(())
}

fn verify_saved_preview(
    address: &str,
    fixture: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (status, file) = http_request(
        address,
        "GET",
        "/api/file?path=pages%2Findex%2Findex.js",
        "",
    )?;
    assert_eq!(status, 200);
    assert!(
        serde_json::from_str::<serde_json::Value>(&file)?["content"]
            .as_str()
            .is_some_and(|content| content.contains("Hello, Mini Program Runtime"))
    );
    let source = std::fs::read_to_string(fixture.join("pages/index/index.js"))?
        .replace("Hello, Mini Program Runtime", "Workbench Saved");
    let payload = serde_json::json!({"path":"pages/index/index.js", "content":source});
    let (status, saved) = http_request(address, "PUT", "/api/file", &payload.to_string())?;
    assert_eq!(status, 200);
    assert!(
        serde_json::from_str::<serde_json::Value>(&saved)?["saved"]
            .as_bool()
            .unwrap_or(false)
    );
    let (status, preview) = http_request(
        address,
        "GET",
        "/api/preview?page=pages%2Findex%2Findex&device=phone",
        "",
    )?;
    assert_eq!(status, 200);
    assert!(preview.contains(".canvas::-webkit-scrollbar{display:none}"));
    assert!(preview.contains("type:'inspector-hide'"));
    assert!(preview.contains("document.addEventListener('contextmenu'"));
    assert!(
        preview.contains("html,body,.shell,.shell main,.page{display:block;width:100%;height:100%")
    );
    assert!(!preview.contains("function onTap"));
    assert!(preview.contains("Workbench Saved"));
    assert_eq!(
        std::fs::read_to_string(fixture.join("pages/index/index.js"))?,
        source
    );
    assert_eq!(
        http_request(address, "GET", "/api/file?path=..%2Fsecret.txt", "")?.0,
        400
    );
    Ok(())
}

fn verify_security_and_switching(
    address: &str,
    second_fixture: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::json!({"path":"pages/index/index.js", "content":"unchanged"});
    assert_eq!(
        http_request_with_origin(
            address,
            "PUT",
            "/api/file",
            &payload.to_string(),
            "http://evil.example"
        )?
        .0,
        403
    );
    let canonical = std::fs::canonicalize(second_fixture)?;
    let payload = serde_json::json!({"path":canonical.to_string_lossy()});
    let (status, switched) = http_request(address, "PUT", "/api/project", &payload.to_string())?;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&switched)?["rootName"],
        second_fixture
            .file_name()
            .ok_or("missing project name")?
            .to_string_lossy()
            .as_ref()
    );
    let (status, current) = http_request(address, "GET", "/api/project", "")?;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&current)?["rootPath"],
        canonical.to_string_lossy().as_ref()
    );
    let (status, preview) = http_request(address, "GET", "/api/preview?device=phone", "")?;
    assert_eq!(status, 200);
    assert!(preview.contains("Second Mini Program"));
    Ok(())
}
fn http_request(
    address: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<(u16, String), Box<dyn std::error::Error>> {
    http_request_with_origin(address, method, path, body, &format!("http://{address}"))
}

fn http_request_with_origin(
    address: &str,
    method: &str,
    path: &str,
    body: &str,
    origin: &str,
) -> Result<(u16, String), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(address)?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    )?;
    writeln!(stream, "Origin: {origin}\r")?;
    if !body.is_empty() {
        stream.write_all(b"Content-Type: application/json\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    stream.write_all(body.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or("invalid HTTP response")?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .ok_or("missing HTTP status")?
        .parse()?;
    Ok((status, body.to_owned()))
}

#[test]
fn parses_example_wxml_through_the_command_entrypoint() -> Result<(), Box<dyn std::error::Error>> {
    let template = example_project().join("pages/index/index.wxml");
    let output = cli().arg("parse-wxml").arg(template).output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("tag_name: \"view\""));
    Ok(())
}
