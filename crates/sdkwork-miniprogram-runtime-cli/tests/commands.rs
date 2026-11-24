//! CLI behavior tests through the compiled command entrypoint.

use std::path::PathBuf;
use std::process::Command;

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

    assert!(output.status.success());
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(document["state"], "Visible");
    assert_eq!(document["pages"][0]["route"], "pages/index/index");
    assert_eq!(document["capabilities"]["wxmlStructuralParsing"], "partial");
    assert_eq!(document["capabilities"]["pixelRendering"], "unsupported");
    assert_eq!(document["pages"][0]["renderNodeCount"], 5);
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
fn parses_example_wxml_through_the_command_entrypoint() -> Result<(), Box<dyn std::error::Error>> {
    let template = example_project().join("pages/index/index.wxml");
    let output = cli().arg("parse-wxml").arg(template).output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("tag_name: \"view\""));
    Ok(())
}
