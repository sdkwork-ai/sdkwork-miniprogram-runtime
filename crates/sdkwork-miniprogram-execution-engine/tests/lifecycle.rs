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
fn runtime_configuration_rejects_zero_resource_ceilings() -> Result<(), RuntimeError> {
    let mut config = RuntimeConfig::default();
    config.resource_budget = ResourceBudget {
        max_applications: 0,
        ..ResourceBudget::default()
    };
    let result = RuntimeBuilder::new().config(config).build();
    assert!(matches!(
        result,
        Err(RuntimeError::InvalidConfiguration {
            field: "max_applications",
            ..
        })
    ));
    Ok(())
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
    let mut config = RuntimeConfig::default();
    config.resource_budget.max_applications = 1;
    config.resource_budget.max_storage_entries_per_application = 1;
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
