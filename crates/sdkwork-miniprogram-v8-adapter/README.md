# V8 Adapter

Provides one-shot JavaScript evaluation using the V8 Rust bindings.

The public API accepts source text, enforces a UTF-8 byte limit, creates an isolate and context, compiles and runs the script, converts its result to a string, and releases the isolate when the call returns. Each call has independent JavaScript state.

```rust
let value = sdkwork_miniprogram_v8_adapter::evaluate("1 + 2")?;
assert_eq!(value, "3");
# Ok::<(), sdkwork_miniprogram_v8_adapter::V8Error>(())
```

This adapter does not provide persistent app contexts, execution timeouts, module loading, wx APIs, Inspector, or operating-system process isolation. The V8 binary distribution and build policy needs platform-specific verification before a platform can be declared supported.
