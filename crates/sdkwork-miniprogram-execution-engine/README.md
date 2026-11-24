# Mini Program Execution Engine

Owns validated runtime configuration, application manifest validation, deterministic application lifecycle transitions, per-application quota-limited storage and virtual files, a bounded page stack, and lazy persistent V8 application contexts.

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

`evaluate_javascript` lazily creates an isolated V8 context per app. Its globals persist through reload and the context is released on unload or destroy. Source size, heap size and execution time are bounded by `ResourceBudget`. `register_javascript_source` executes source files with registration shims for `App`, `Page`, `Component` and `console`. JavaScript console calls are retained in a bounded V8 buffer and transferred to a bounded, per-application Runtime debug journal together with lifecycle and host-published network events. `debug_events` supports cursor reads, and `clear_debug_events` clears retained events without resetting their sequence. A host-backed request provider may publish redacted request lifecycle events with `record_network_event`. The crate does not implement `wx.request`, a Network provider, a CDP gateway or a debugger frontend. Hosts can invoke registered app and page lifecycle methods. These shims do not supply CommonJS/ESM, wx APIs, observer or component framework behavior. Page navigation provides a tested registered-route stack with `navigate_to`, `redirect_to`, and `navigate_back`. It does not implement `switchTab`, `reLaunch`, query parameters, full route lifecycle transitions, or synchronization with a visual container. Storage and the virtual filesystem are in-memory app-isolation primitives and do not persist across runtime instances. Virtual file paths reject absolute, traversal, backslash, empty and dot components. File count, single-file size and aggregate bytes are bounded by `ResourceBudget`.
