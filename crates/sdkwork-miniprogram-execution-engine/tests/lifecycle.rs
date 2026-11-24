//! Public lifecycle and application isolation acceptance tests.

use sdkwork_miniprogram_execution_engine::{
    ApplicationManifest, ApplicationState, ResourceBudget, Runtime, RuntimeBuilder, RuntimeConfig,
    RuntimeError,
};

fn manifest(app_id: &str) -> ApplicationManifest {
    ApplicationManifest {
        app_id: app_id.to_owned(),
        version: "1.0.0".to_owned(),
        entry_page: "pages/home/index".to_owned(),
        pages: vec!["pages/home/index".to_owned()],
    }
}

fn runtime() -> Result<Runtime, RuntimeError> {
    RuntimeBuilder::new().build()
}

#[test]
fn runtime_configuration_rejects_zero_resource_ceilings() {
    let config = RuntimeConfig {
        resource_budget: ResourceBudget {
            max_applications: 0,
            ..ResourceBudget::default()
        },
        ..RuntimeConfig::default()
    };
    let result = RuntimeBuilder::new().config(config).build();
    assert!(matches!(
        result,
        Err(RuntimeError::InvalidConfiguration {
            field: "max_applications",
            ..
        })
    ));
}

#[test]
fn application_lifecycle_has_deterministic_transitions() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let app = runtime.load_application(manifest("shop"))?;
    assert_eq!(app.state()?, ApplicationState::Loaded);
    app.launch()?;
    app.show()?;
    app.hide()?;
    app.show()?;
    app.reload()?;
    assert_eq!(app.state()?, ApplicationState::Launched);
    runtime.unload_application("shop")?;
    assert_eq!(app.state()?, ApplicationState::Unloaded);
    Ok(())
}

#[test]
fn applications_have_independent_storage_namespaces() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let first = runtime.load_application(manifest("first"))?;
    let second = runtime.load_application(manifest("second"))?;
    first.storage().set("token", "secret")?;
    assert_eq!(first.storage().get("token")?.as_deref(), Some("secret"));
    assert_eq!(second.storage().get("token")?, None);
    Ok(())
}

#[test]
fn applications_have_independent_virtual_filesystems_and_unload_closes_handles()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = runtime()?;
    let first = runtime.load_application(manifest("files-first"))?;
    let second = runtime.load_application(manifest("files-second"))?;
    let first_files = first.file_system();
    let second_files = second.file_system();
    first_files.write_file("documents/readme.txt", b"private")?;

    assert_eq!(
        first_files.read_file("documents/readme.txt")?,
        Some(b"private".to_vec())
    );
    assert_eq!(second_files.read_file("documents/readme.txt")?, None);
    runtime.unload_application("files-first")?;
    assert!(matches!(
        first_files.read_file("documents/readme.txt"),
        Err(RuntimeError::ApplicationFileSystemClosed { .. })
    ));
    assert_eq!(second_files.file_count()?, 0);
    Ok(())
}

#[test]
fn runtime_applies_virtual_filesystem_budgets() -> Result<(), Box<dyn std::error::Error>> {
    let config = RuntimeConfig {
        resource_budget: ResourceBudget {
            max_files_per_application: 1,
            max_file_bytes: 4,
            max_total_file_bytes: 5,
            ..ResourceBudget::default()
        },
        ..RuntimeConfig::default()
    };
    let runtime = RuntimeBuilder::new().config(config).build()?;
    let app = runtime.load_application(manifest("files-quota"))?;
    let files = app.file_system();
    files.write_file("first.bin", b"1234")?;
    assert!(matches!(
        files.write_file("first.bin", b"12345"),
        Err(RuntimeError::FileSystemQuotaExceeded {
            quota: "max_file_bytes",
            ..
        })
    ));
    assert!(matches!(
        files.write_file("second.bin", b"x"),
        Err(RuntimeError::FileSystemQuotaExceeded {
            quota: "max_files_per_application",
            ..
        })
    ));
    assert_eq!(files.total_bytes()?, 4);
    Ok(())
}

#[test]
fn application_javascript_state_persists_and_is_isolated() -> Result<(), Box<dyn std::error::Error>>
{
    let runtime = runtime()?;
    let first = runtime.load_application(manifest("js-first"))?;
    let second = runtime.load_application(manifest("js-second"))?;
    assert_eq!(first.evaluate_javascript("globalThis.value = 40")?, "40");
    assert_eq!(first.evaluate_javascript("globalThis.value + 2")?, "42");
    assert_eq!(
        second.evaluate_javascript("typeof globalThis.value")?,
        "undefined"
    );
    first.launch()?;
    first.reload()?;
    assert_eq!(first.evaluate_javascript("globalThis.value")?, "40");
    runtime.unload_application("js-first")?;
    assert!(matches!(
        first.evaluate_javascript("1"),
        Err(RuntimeError::InvalidState { .. })
    ));
    Ok(())
}

#[test]
fn manifest_rejects_path_traversal_and_unknown_entry_page() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let mut unsafe_manifest = manifest("unsafe");
    unsafe_manifest.pages = vec!["../outside".to_owned()];
    unsafe_manifest.entry_page = "../outside".to_owned();
    let unsafe_result = runtime.load_application(unsafe_manifest);
    assert!(matches!(
        unsafe_result,
        Err(RuntimeError::InvalidManifest { field: "pages", .. })
    ));

    let mut invalid_entry = manifest("invalid-entry");
    invalid_entry.entry_page = "pages/missing".to_owned();
    let entry_result = runtime.load_application(invalid_entry);
    assert!(matches!(
        entry_result,
        Err(RuntimeError::InvalidManifest {
            field: "entry_page",
            ..
        })
    ));
    Ok(())
}

#[test]
fn runtime_enforces_application_and_storage_limits() -> Result<(), RuntimeError> {
    let config = RuntimeConfig {
        resource_budget: ResourceBudget {
            max_applications: 1,
            max_storage_entries_per_application: 1,
            ..ResourceBudget::default()
        },
        ..RuntimeConfig::default()
    };
    let runtime = RuntimeBuilder::new().config(config).build()?;
    let first = runtime.load_application(manifest("first"))?;
    let second_result = runtime.load_application(manifest("second"));
    assert!(matches!(
        second_result,
        Err(RuntimeError::InvalidConfiguration {
            field: "max_applications",
            ..
        })
    ));
    first.storage().set("one", "1")?;
    let storage_result = first.storage().set("two", "2");
    assert!(matches!(
        storage_result,
        Err(RuntimeError::InvalidConfiguration {
            field: "storage_entries",
            ..
        })
    ));
    Ok(())
}

#[test]
fn destroyed_handle_cannot_be_reused() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let app = runtime.load_application(manifest("destroyed"))?;
    runtime.destroy_application("destroyed")?;
    assert_eq!(app.state()?, ApplicationState::Destroyed);
    let result = app.launch();
    assert!(matches!(
        result,
        Err(RuntimeError::ApplicationDestroyed { .. })
    ));
    Ok(())
}

#[test]
fn unloading_releases_existing_storage_handles() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let app = runtime.load_application(manifest("released"))?;
    let storage = app.storage();
    storage.set("key", "value")?;

    runtime.unload_application("released")?;

    assert!(matches!(
        storage.get("key"),
        Err(RuntimeError::ApplicationStorageClosed { .. })
    ));
    Ok(())
}

#[test]
fn page_stack_navigation_checks_routes_and_configured_depth() -> Result<(), RuntimeError> {
    let config = RuntimeConfig {
        resource_budget: ResourceBudget {
            max_pages_per_application: 2,
            ..ResourceBudget::default()
        },
        ..RuntimeConfig::default()
    };
    let runtime = RuntimeBuilder::new().config(config).build()?;
    let mut app_manifest = manifest("navigation");
    app_manifest.pages.push("pages/profile/index".to_owned());
    let app = runtime.load_application(app_manifest)?;
    assert!(matches!(
        app.navigate_to("pages/profile/index"),
        Err(RuntimeError::InvalidState {
            operation: "navigate_to",
            ..
        })
    ));

    app.launch()?;
    assert_eq!(
        app.navigate_to("pages/profile/index")?,
        "pages/profile/index"
    );
    assert_eq!(
        app.page_stack()?,
        vec!["pages/home/index", "pages/profile/index"]
    );
    assert!(matches!(
        app.navigate_to("pages/home/index"),
        Err(RuntimeError::NavigationFailed { .. })
    ));
    assert_eq!(app.navigate_back(1)?, "pages/home/index");
    assert!(matches!(
        app.navigate_back(1),
        Err(RuntimeError::NavigationFailed { .. })
    ));
    assert!(matches!(
        app.navigate_to("pages/unregistered/index"),
        Err(RuntimeError::NavigationFailed { .. })
    ));
    Ok(())
}

#[test]
fn redirect_replaces_current_page_without_growing_the_stack() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let mut app_manifest = manifest("redirect");
    app_manifest.entry_page = "pages/home/index".to_owned();
    app_manifest.pages = vec![
        "pages/home/index".to_owned(),
        "pages/profile/index".to_owned(),
        "pages/settings/index".to_owned(),
    ];
    let app = runtime.load_application(app_manifest)?;
    app.launch()?;
    app.navigate_to("pages/profile/index")?;
    assert_eq!(
        app.redirect_to("pages/settings/index")?,
        "pages/settings/index"
    );
    assert_eq!(
        app.page_stack()?,
        vec!["pages/home/index", "pages/settings/index"]
    );
    Ok(())
}

#[test]
fn unload_clears_the_page_stack_and_blocks_further_navigation() -> Result<(), RuntimeError> {
    let runtime = runtime()?;
    let app = runtime.load_application(manifest("unloaded-stack"))?;
    app.launch()?;
    runtime.unload_application("unloaded-stack")?;
    assert_eq!(app.page_stack()?.len(), 0);
    assert!(matches!(
        app.navigate_back(1),
        Err(RuntimeError::InvalidState { .. })
    ));
    Ok(())
}
