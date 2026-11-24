//! Static, read-only Windows preview for the supported mini-program subset.

use std::env;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

/// Data needed to render one parsed page in the static preview.
#[derive(Debug, Clone)]
pub struct ViewerPage {
    /// Route shown in the preview navigation.
    pub route: String,
    /// Data-evaluated structural tree serialized as JSON.
    pub render_tree: Value,
    /// Computed declarations from the supported WXSS subset.
    pub computed_styles: Value,
    /// Source-located style diagnostics.
    pub style_diagnostics: Value,
    /// Snapshot of evaluated page data.
    pub page_data: Value,
    /// Runtime-owned console records collected while loading the page.
    pub console_events: Value,
    /// Runtime-owned network records collected while loading the page.
    pub network_events: Value,
    /// Target identity and lifecycle state reported by the Runtime.
    pub target: Value,
    /// Runtime-owned debugger protocol capability report.
    pub debug_capabilities: Value,
}

/// Launch failures for the native preview window.
#[derive(Debug)]
pub enum ViewerError {
    /// No supported Chromium executable was found on Windows.
    BrowserNotFound,
    /// The browser process could not be started.
    Launch(io::Error),
}

impl std::fmt::Display for ViewerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BrowserNotFound => formatter.write_str(
                "Microsoft Edge or Google Chrome was not found; install a Chromium browser to launch the Windows preview",
            ),
            Self::Launch(error) => write!(formatter, "could not launch the Windows preview window: {error}"),
        }
    }
}

impl std::error::Error for ViewerError {}

/// Renders one page into a standalone document for an accurately sized device viewport.
#[must_use]
pub fn generate_page_html(title: &str, page: &ViewerPage) -> String {
    let mut html = String::with_capacity(12_000);
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    html.push_str("<title>");
    push_escaped(&mut html, title);
    html.push_str(" · ");
    push_escaped(&mut html, &page.route);
    html.push_str("</title><style>");
    html.push_str(STYLESHEET);
    html.push_str("html,body,.shell,.shell main,.page{display:block;width:100%;height:100%;min-width:0;min-height:0;margin:0;padding:0;overflow:hidden}body{background:#fff}.shell{max-width:none}.page{border:0;border-radius:0;box-shadow:none}.page-title{display:none}.canvas{display:block;width:100%;height:100%;min-height:0;margin:0;padding:18px;overflow:auto;background:#fff;scrollbar-width:none}.canvas::-webkit-scrollbar{display:none}.node.is-inspected{outline:2px solid #3984ff;outline-offset:2px}footer,.routes,header{display:none}</style></head><body><div class=\"shell\"><main>");
    html.push_str("<section class=\"page\"><div class=\"canvas\">");
    if let Some(nodes) = page.render_tree.get("children").and_then(Value::as_array) {
        for node in nodes {
            render_node(&mut html, node, &page.computed_styles);
        }
    }
    html.push_str("</div>");
    html.push_str(
        "</section></main></div><script id=\"inspector-data\" type=\"application/json\">",
    );
    let inspector = serde_json::json!({
        "route": page.route,
        "pageData": page.page_data,
        "console": page.console_events,
        "network": page.network_events,
        "target": page.target,
        "protocolVersion": "sdkwork-miniprogram-inspector/0.1",
        "debugCapabilities": page.debug_capabilities,
        "renderTree": page.render_tree,
        "computedStyles": page.computed_styles,
        "diagnostics": page.style_diagnostics
    });
    let serialized = serde_json::to_string(&inspector).unwrap_or_else(|_| "{}".to_owned());
    for character in serialized.chars() {
        match character {
            '<' => html.push_str("\\u003c"),
            '>' => html.push_str("\\u003e"),
            '&' => html.push_str("\\u0026"),
            _ => html.push(character),
        }
    }
    html.push_str("</script><script>document.addEventListener('click',event=>{const node=event.target.closest('[data-node-id]');if(!node)return;document.querySelectorAll('.is-inspected').forEach(item=>item.classList.remove('is-inspected'));node.classList.add('is-inspected');parent.postMessage({source:'sdkwork-preview',type:'node-selected',nodeId:Number(node.dataset.nodeId)},location.origin)});document.addEventListener('contextmenu',event=>{event.preventDefault();parent.postMessage({source:'sdkwork-preview',type:'inspector-hide'},location.origin)})</script></body></html>");
    html
}

/// Launch a URL in the platform browser's app window when supported.
pub fn launch_url(url: &str) -> Result<(), ViewerError> {
    #[cfg(windows)]
    {
        let browser = find_windows_browser().ok_or(ViewerError::BrowserNotFound)?;
        Command::new(browser)
            .arg(format!("--app={url}"))
            .arg("--new-window")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(ViewerError::Launch)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(opener)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(ViewerError::Launch)?;
        Ok(())
    }
}

#[cfg(windows)]
fn find_windows_browser() -> Option<PathBuf> {
    let roots = [
        env::var_os("ProgramFiles(x86)"),
        env::var_os("ProgramFiles"),
        env::var_os("LOCALAPPDATA"),
    ];
    let relative_paths = [
        Path::new("Microsoft/Edge/Application/msedge.exe"),
        Path::new("Google/Chrome/Application/chrome.exe"),
    ];
    for root in roots.into_iter().flatten() {
        for relative in relative_paths {
            let candidate = PathBuf::from(&root).join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for executable in ["msedge.exe", "chrome.exe"] {
        let output = Command::new("where.exe").arg(executable).output().ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())?;
            return Some(PathBuf::from(line));
        }
    }
    None
}

fn render_node(output: &mut String, node: &Value, computed_styles: &Value) {
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "text" => {
            output.push_str("<span class=\"render-text\"");
            push_node_metadata(output, node, "text");
            output.push('>');
            push_escaped(
                output,
                node.get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            output.push_str("</span>");
        }
        "binding" => {
            output.push_str("<span class=\"binding\"");
            push_node_metadata(output, node, "binding");
            output.push_str(">{{");
            push_escaped(
                output,
                node.get("expression")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            output.push_str("}}</span>");
        }
        "element" => {
            let tag = safe_tag(
                node.get("tag_name")
                    .and_then(Value::as_str)
                    .unwrap_or("view"),
            );
            output.push('<');
            output.push_str(tag);
            let mut classes = String::from("node");
            if tag == "button" {
                classes.push_str(" button-node");
            }
            if let Some(class) = node
                .get("attributes")
                .and_then(|attributes| attributes.get("class"))
                .and_then(attribute_text)
                && !class.is_empty()
            {
                classes.push(' ');
                classes.push_str(&class);
            }
            push_attribute(output, "class", &classes);
            push_node_metadata(output, node, "element");
            if let Some(attributes) = node.get("attributes").and_then(Value::as_object) {
                for (name, value) in attributes {
                    if name.starts_with("wx:")
                        || name.starts_with("bind")
                        || name.starts_with("catch")
                        || name == "class"
                        || !safe_attribute_name(name)
                    {
                        continue;
                    }
                    let Some(value) = attribute_text(value) else {
                        continue;
                    };
                    output.push(' ');
                    push_attribute(output, name, &value);
                }
            }
            if let Some(id) = node.get("id").and_then(Value::as_u64)
                && let Some(style) = computed_styles
                    .get(id.to_string())
                    .and_then(Value::as_object)
            {
                let mut declarations = String::new();
                for (property, value) in style {
                    if let Some(value) = value.as_str()
                        && valid_css_token(property)
                        && valid_css_token(value)
                    {
                        let _ = write!(declarations, "{property}:{value};");
                    }
                }
                if !declarations.is_empty() {
                    push_attribute(output, "style", &declarations);
                }
            }
            output.push('>');
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for child in children {
                    render_node(output, child, computed_styles);
                }
            }
            output.push_str("</");
            output.push_str(tag);
            output.push('>');
        }
        _ => {}
    }
}

fn push_node_metadata(output: &mut String, node: &Value, kind: &str) {
    if let Some(id) = node.get("id").and_then(Value::as_u64) {
        let _ = write!(output, " data-node-id=\"{id}\" data-node-kind=\"{kind}\"");
    }
    if let Some(source) = node.get("source") {
        if let Some(line) = source.get("line").and_then(Value::as_u64) {
            let _ = write!(output, " data-source-line=\"{line}\"");
        }
        if let Some(column) = source.get("column").and_then(Value::as_u64) {
            let _ = write!(output, " data-source-column=\"{column}\"");
        }
    }
}

fn attribute_text(value: &Value) -> Option<String> {
    match value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "boolean" => Some(String::new()),
        "literal" | "binding" => value
            .get("value")
            .or_else(|| value.get("expression"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        "interpolated" => value.get("parts").and_then(Value::as_array).map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    part.get("value")
                        .or_else(|| part.get("expression"))
                        .and_then(Value::as_str)
                })
                .collect::<Vec<_>>()
                .join("")
        }),
        _ => None,
    }
}

fn push_attribute(output: &mut String, name: &str, value: &str) {
    output.push_str(name);
    output.push_str("=\"");
    push_escaped(output, value);
    output.push('"');
}

fn push_escaped(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

fn safe_tag(tag: &str) -> &'static str {
    match tag.to_ascii_lowercase().as_str() {
        "view" | "text" | "button" | "image" | "scroll-view" | "swiper" | "navigator" => {
            tag_name(tag)
        }
        _ => "div",
    }
}

fn tag_name(tag: &str) -> &'static str {
    match tag.to_ascii_lowercase().as_str() {
        "text" => "span",
        "button" => "button",
        "image" => "img",
        "navigator" => "a",
        _ => "div",
    }
}

fn valid_css_token(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    !value.is_empty()
        && !value.contains(['<', '>', '"', '\'', '{', '}', ';'])
        && !normalized.contains("url(")
        && !normalized.contains("expression(")
        && !normalized.contains("javascript:")
}

fn safe_attribute_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "class" | "id" | "title" | "alt" | "role" | "aria-label" | "type" | "disabled"
    )
}

const STYLESHEET: &str = r#"
:root { color-scheme: light; font-family: "Segoe UI", sans-serif; background: #f3f5f7; color: #1b2430; }
* { box-sizing: border-box; }
body { margin: 0; min-width: 760px; }
.shell { max-width: 1120px; margin: 0 auto; padding: 32px 40px 28px; }
header { display: flex; align-items: flex-start; justify-content: space-between; gap: 24px; padding-bottom: 24px; border-bottom: 1px solid #d9e0e8; }
.eyebrow { margin: 0 0 8px; color: #54708c; font-size: 12px; font-weight: 700; letter-spacing: .12em; }
h1, h2, p { margin-top: 0; }
h1 { margin-bottom: 6px; font-size: 30px; font-weight: 650; letter-spacing: -.02em; }
h2 { margin-bottom: 0; font-size: 17px; font-weight: 650; }
.subtle { margin-bottom: 0; color: #697586; font-size: 14px; }
.status, .chip { display: inline-flex; align-items: center; border: 1px solid #bad7c5; border-radius: 999px; color: #23613b; background: #eef8f1; padding: 5px 10px; font-size: 11px; font-weight: 700; letter-spacing: .08em; }
.routes { display: flex; align-items: center; gap: 10px; min-height: 58px; border-bottom: 1px solid #e1e6eb; }
.routes .label { color: #697586; font-size: 12px; font-weight: 700; margin-right: 6px; }
.route { border: 1px solid #ced8e2; border-radius: 6px; background: #fff; padding: 7px 10px; color: #34495e; font: 12px ui-monospace, SFMono-Regular, Consolas, monospace; }
main { display: grid; gap: 18px; padding-top: 22px; }
.page { overflow: hidden; border: 1px solid #d9e0e8; border-radius: 8px; background: #fff; box-shadow: 0 8px 24px #243b530d; }
.page-title { display: flex; align-items: center; gap: 10px; padding: 16px 18px; border-bottom: 1px solid #edf0f3; }
.canvas { min-height: 230px; padding: 28px; background: #fbfcfd; }
.node { display: block; }
.button-node { display: inline-block; border: 0; border-radius: 6px; background: #1769aa; color: white; padding: 9px 16px; font: inherit; }
.binding { color: #9b4dca; background: #f8effc; border-radius: 3px; padding: 1px 3px; }
.diagnostics { margin: 0 18px 16px; color: #8a4b08; font-size: 12px; }
.diagnostics summary { cursor: pointer; }
.diagnostics ul { margin-bottom: 0; }
.empty { border: 1px dashed #bdc9d5; border-radius: 8px; padding: 36px; color: #697586; text-align: center; }
footer { margin-top: 24px; border-top: 1px solid #d9e0e8; padding-top: 16px; color: #697586; font-size: 12px; line-height: 1.55; }
"#;

#[cfg(test)]
mod tests {
    use super::{ViewerPage, generate_page_html};
    use serde_json::json;

    #[test]
    fn escapes_project_values_and_keeps_preview_static() {
        let html = generate_page_html(
            "<unsafe>",
            &ViewerPage {
                route: "pages/<x>".to_owned(),
                render_tree: json!({
                    "children": [{
                        "kind": "element", "id": 0, "tag_name": "view",
                        "attributes": {
                            "title": {"kind": "literal", "value": "<x>"},
                            "onclick": {"kind": "literal", "value": "alert(1)"}
                        },
                        "children": [{"kind": "text", "id": 1, "value": "<script>alert(1)</script>", "source": {"start": 0, "end": 4, "line": 1, "column": 1}}]
                    }]
                }),
                computed_styles: json!({"0": {"color": "red"}}),
                style_diagnostics: json!([]),
                page_data: json!({}),
                console_events: json!([]),
                network_events: json!([]),
                target: json!({"targetId":"test","type":"mini-program"}),
                debug_capabilities: json!({}),
            },
        );
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("onclick="));
        assert!(html.contains("data-node-id=\"0\""));
        assert!(html.contains("data-source-line=\"1\""));
        assert!(html.contains("scrollbar-width:none"));
        assert!(html.contains("pages/&lt;x&gt;"));
    }
}
