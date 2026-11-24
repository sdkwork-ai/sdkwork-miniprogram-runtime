//! Command-line entrypoint for mini-program project and runtime diagnostics.

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

mod viewer;
mod workbench;
mod workbench_host;

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
        "run" => {
            let mut project_path = None;
            let mut launch_window = true;
            for argument in arguments {
                if argument == "--no-launch" {
                    launch_window = false;
                } else if project_path.replace(PathBuf::from(argument)).is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "run accepts one project directory",
                    )
                    .into());
                }
            }
            let path = required_path(
                project_path.map(|path| path.display().to_string()),
                "project directory",
            )?;
            run_project(&path, launch_window)
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
        "  run <project-dir> [--no-launch]  Start the editor and device preview workbench"
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
    register_project_scripts(&application, &project)?;
    application.launch()?;
    let app_on_launch = application.invoke_app_lifecycle("onLaunch")?;
    application.show()?;
    let app_on_show = application.invoke_app_lifecycle("onShow")?;
    let mut page_results = Vec::with_capacity(project.pages.len());
    for (page_index, page) in project.pages.iter().enumerate() {
        page_results.push(inspect_page(&application, &project, page, page_index)?);
    }
    let output = serde_json::json!({
        "projectRoot": project.root,
        "state": format!("{:?}", application.state()?),
        "appScriptAvailable": project.app_script_path.exists(),
        "appStyleAvailable": project.app_style_path.is_some(),
        "pages": page_results,
        "appLifecycle": {
            "onLaunchResult": app_on_launch,
            "onShowResult": app_on_show
        },
        "javascriptRegistrations": application.javascript_registration_snapshot()?,
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

fn run_project(root: &std::path::Path, launch_window: bool) -> Result<(), Box<dyn Error>> {
    use sdkwork_miniprogram_package_loader::load_project;

    let project = match load_project(root) {
        Ok(project) => project,
        Err(sdkwork_miniprogram_package_loader::LoaderError::InvalidProject { diagnostics }) => {
            let details = serde_json::to_string_pretty(&diagnostics).map_err(io::Error::other)?;
            return Err(io::Error::new(io::ErrorKind::InvalidData, details).into());
        }
        Err(error) => return Err(io::Error::other(format!("{error:?}")).into()),
    };
    let title = project
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Mini Program");
    workbench_host::serve(&project.root, title, launch_window)
}

fn register_project_scripts(
    application: &sdkwork_miniprogram_execution_engine::ApplicationHandle,
    project: &sdkwork_miniprogram_package_loader::MiniProgramProject,
) -> Result<(), Box<dyn Error>> {
    let source = String::from_utf8(
        project.read_file(project.app_script_path.strip_prefix(&project.root)?)?,
    )?;
    application.register_javascript_source("app.js", &source)?;
    for page in &project.pages {
        let source =
            String::from_utf8(project.read_file(page.script_path.strip_prefix(&project.root)?)?)?;
        application.register_javascript_source(&page.route, &source)?;
    }
    Ok(())
}

fn inspect_page(
    application: &sdkwork_miniprogram_execution_engine::ApplicationHandle,
    project: &sdkwork_miniprogram_package_loader::MiniProgramProject,
    page: &sdkwork_miniprogram_package_loader::PageDefinition,
    page_index: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
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
    let registrations = application.javascript_registration_snapshot()?;
    let registered_pages = registrations["pages"].as_array();
    let registered_page = registered_pages.and_then(|pages| pages.get(page_index));
    let has_registered_page = registered_page.is_some();
    let page_on_load = if has_registered_page {
        application.invoke_page_lifecycle(page_index, "onLoad")?
    } else {
        None
    };
    let page_on_show = if has_registered_page {
        application.invoke_page_lifecycle(page_index, "onShow")?
    } else {
        None
    };
    let page_data_after_lifecycle = application.javascript_registration_snapshot()?["pages"]
        .as_array()
        .and_then(|pages| pages.get(page_index))
        .and_then(|page| page.get("data"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let data_render = sdkwork_miniprogram_headless_renderer::build_with_data(
        &tree,
        page.template_path.display().to_string(),
        &page_data_after_lifecycle,
        1_000,
    );
    let javascript_bytes = project
        .read_file(page.script_path.strip_prefix(&project.root)?)?
        .len();
    Ok(serde_json::json!({
        "route": page.route,
        "wxmlNodes": tree.children.len(),
        "renderNodeCount": styled_tree.tree.node_count,
        "renderTree": styled_tree.tree,
        "dataRenderTree": data_render.tree,
        "dataRenderDiagnostics": data_render.diagnostics,
        "pageData": page_data_after_lifecycle,
        "computedStyles": styled_tree.computed_styles,
        "styleDiagnostics": styled_tree.diagnostics,
        "lifecycle": {
            "onLoadResult": page_on_load,
            "onShowResult": page_on_show
        },
        "wxssAvailable": page.style_path.is_some(),
        "javascriptBytes": javascript_bytes
    }))
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
