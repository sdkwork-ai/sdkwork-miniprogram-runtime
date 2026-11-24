# Mini Program Execution Engine

Owns validated runtime configuration, application manifest validation, deterministic application lifecycle transitions, and per-application quota-limited in-memory storage.

```rust
use sdkwork_miniprogram_execution_engine::{ApplicationManifest, RuntimeBuilder};

let runtime = RuntimeBuilder::new().build()?;
let application = runtime.load_application(ApplicationManifest {
    app_id: "example".to_owned(),
    version: "1.0.0".to_owned(),
    entry_page: "pages/index/index".to_owned(),
    pages: vec!["pages/index/index".to_owned()],
})?;
application.launch()?;
application.show()?;
# Ok::<(), sdkwork_miniprogram_execution_engine::RuntimeError>(())
```

The crate does not yet bind persistent V8 contexts, page routing, renderers, host APIs, files, network policies, or debug sessions. Storage is an in-memory isolation primitive and does not persist across runtime instances.
