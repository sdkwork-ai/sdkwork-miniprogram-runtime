# V8 Adapter

Provides one-shot and persistent isolated JavaScript evaluation using the V8 Rust bindings.

The `evaluate` API accepts source text, enforces UTF-8 byte and V8 heap limits, creates an isolate and context, compiles and runs the script, converts its result to a string, and releases the isolate when the call returns. Each call has independent JavaScript state. `JavaScriptContext` owns a persistent isolate and context for one application, with quiet `console` and registration-only `App`, `Page` and `Component` globals. Calls on it are serialized on its V8 thread, and its globals persist until the context is dropped. Each context enforces UTF-8 source, V8 heap, and execution-time limits. A timed-out context is terminated and cannot be reused.

```rust
let value = sdkwork_miniprogram_v8_adapter::evaluate("1 + 2")?;
assert_eq!(value, "3");
# Ok::<(), sdkwork_miniprogram_v8_adapter::V8Error>(())
```

Persistent use:

```rust
let mut app_context = sdkwork_miniprogram_v8_adapter::JavaScriptContext::new()?;
app_context.evaluate("globalThis.count = 1")?;
assert_eq!(app_context.evaluate("++globalThis.count")?, "2");
# Ok::<(), sdkwork_miniprogram_v8_adapter::V8Error>(())
```

`register_source` records app/page/component definitions and includes the project-relative source name in compilation and runtime failures. `registration_snapshot` returns JSON-compatible metadata, and the callback methods invoke explicitly named lifecycle functions. These facilities do not provide the module loader or mini-program framework behavior around the definitions.

This adapter does not provide module loading, wx APIs, Inspector, or operating-system process isolation. Hosts that run untrusted applications need an operating-system isolation boundary. The V8 binary distribution and build policy needs platform-specific verification before a platform can be declared supported.
