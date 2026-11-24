# Mini Program Execution Engine

Owns validated runtime configuration, application manifest validation, deterministic application lifecycle transitions, per-application quota-limited in-memory storage, a bounded page stack, and lazy persistent V8 application contexts.

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

`evaluate_javascript` lazily creates an isolated V8 context per app. Its globals persist through reload and the context is released on unload or destroy. Source size and execution time are bounded by `ResourceBudget`. CommonJS/ESM loading and wx globals are unsupported. Page navigation provides a tested registered-route stack with `navigate_to`, `redirect_to`, and `navigate_back`. It does not implement `switchTab`, `reLaunch`, query parameters, route lifecycle callbacks, or synchronization with a visual container. Storage is an in-memory isolation primitive and does not persist across runtime instances. The crate does not provide network or filesystem host APIs or debug sessions.
