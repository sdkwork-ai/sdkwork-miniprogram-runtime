//! Command-line entrypoint for mini-program project and runtime diagnostics.

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    initialize_logging();
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".to_owned());
    match command.as_str() {
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => print_version(),
        "inspect" => {
            let path = required_path(arguments.next(), "project directory")?;
            inspect_project(&path)
        }
        "parse-wxml" => {
            let path = required_path(arguments.next(), "WXML file")?;
            parse_wxml(&path)
        }
        "eval-js" => evaluate_javascript(&arguments.collect::<Vec<_>>().join(" ")),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command `{command}`; run with --help"),
        )
        .into()),
    }
}

fn initialize_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn print_help() -> Result<(), Box<dyn Error>> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "SDKWORK Mini Program Runtime CLI")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(
        stdout,
        "  inspect <project-dir>  Validate project files and parse page WXML"
    )?;
    writeln!(
        stdout,
        "  parse-wxml <file>      Parse WXML structure and show its syntax tree"
    )?;
    writeln!(
        stdout,
        "  eval-js <source>       Evaluate JavaScript in an isolated V8 context"
    )?;
    writeln!(
        stdout,
        "  version                Show runtime and CLI versions"
    )?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Full WeChat APIs, evaluated data bindings, pixel rendering, and remote debugging are not yet implemented."
    )?;
    Ok(())
}

fn print_version() -> Result<(), Box<dyn Error>> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "runtime {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(stdout, "compatibility profile wechat-public (partial)")?;
    writeln!(
        stdout,
        "V8 adapter persistent per-app and one-shot contexts"
    )?;
    Ok(())
}

fn inspect_project(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use sdkwork_miniprogram_execution_engine::{ApplicationManifest, RuntimeBuilder};
    use sdkwork_miniprogram_package_loader::load_project;

    let project = match load_project(root) {
        Ok(project) => project,
        Err(sdkwork_miniprogram_package_loader::LoaderError::InvalidProject { diagnostics }) => {
            let details = serde_json::to_string_pretty(&diagnostics).map_err(io::Error::other)?;
            return Err(io::Error::new(io::ErrorKind::InvalidData, details).into());
        }
        Err(error) => return Err(io::Error::other(format!("{error:?}")).into()),
    };
    let pages = project
        .pages
        .iter()
        .map(|page| page.route.clone())
        .collect::<Vec<_>>();
    let entry_page = project
        .manifest
        .entry_page_path
        .clone()
        .or_else(|| pages.first().cloned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "project has no pages"))?;
    let manifest = ApplicationManifest {
        app_id: project
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("inspected-project")
            .to_owned(),
        version: "0.0.0-inspection".to_owned(),
        entry_page,
        pages,
    };
    let runtime = RuntimeBuilder::new().build()?;
    let application = runtime.load_application(manifest)?;
    application.launch()?;
    application.show()?;

    let mut page_results = Vec::with_capacity(project.pages.len());
    for page in &project.pages {
        let source =
            String::from_utf8(project.read_file(page.template_path.strip_prefix(&project.root)?)?)?;
        let tree = sdkwork_miniprogram_wxml_compiler::parse(
            page.template_path.display().to_string(),
            &source,
        )?;
        let style_source = page
            .style_path
            .as_ref()
            .map(|path| {
                let relative_path = path.strip_prefix(&project.root).map_err(io::Error::other)?;
                let source = project.read_file(relative_path).map_err(io::Error::other)?;
                String::from_utf8(source).map_err(io::Error::other)
            })
            .transpose()?;
        let styled_tree = sdkwork_miniprogram_headless_renderer::build_with_styles(
            &tree,
            page.style_path.as_ref().map_or_else(
                || String::from("<missing>"),
                |path| path.display().to_string(),
            ),
            style_source.as_deref().unwrap_or_default(),
        );
        page_results.push(serde_json::json!({
            "route": page.route,
            "wxmlNodes": tree.children.len(),
            "renderNodeCount": styled_tree.tree.node_count,
            "renderTree": styled_tree.tree,
            "computedStyles": styled_tree.computed_styles,
            "styleDiagnostics": styled_tree.diagnostics,
            "wxssAvailable": page.style_path.is_some(),
            "javascriptBytes": project.read_file(
                page.script_path.strip_prefix(&project.root)?
            )?.len()
        }));
    }
    let output = serde_json::json!({
        "projectRoot": project.root,
        "state": format!("{:?}", application.state()?),
        "appScriptAvailable": project.app_script_path.exists(),
        "appStyleAvailable": project.app_style_path.is_some(),
        "pages": page_results,
        "capabilities": {
            "projectInspection": "tested",
            "wxmlStructuralParsing": "partial",
            "headlessStructuralTree": "implemented",
            "javascriptExecution": "persistent per-app V8 with source and deadline limits; unsupported wx APIs and modules",
            "wxApiExecution": "unsupported",
            "wxssStructuralSubset": "partial",
            "stylesLayoutAndPixels": "unsupported",
            "wxApis": "unsupported"
        }
    });
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", serde_json::to_string_pretty(&output)?)?;
    application.hide()?;
    runtime.unload_application(application.app_id())?;
    Ok(())
}

fn parse_wxml(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    let tree = sdkwork_miniprogram_wxml_compiler::parse(path.display().to_string(), &source)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{tree:#?}")?;
    Ok(())
}

fn evaluate_javascript(source: &str) -> Result<(), Box<dyn Error>> {
    if source.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected JavaScript source text",
        )
        .into());
    }
    let result = sdkwork_miniprogram_v8_adapter::evaluate(source)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{result}")?;
    Ok(())
}

fn required_path(value: Option<String>, description: &str) -> Result<PathBuf, Box<dyn Error>> {
    value.map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected a {description} path"),
        )
        .into()
    })
}
