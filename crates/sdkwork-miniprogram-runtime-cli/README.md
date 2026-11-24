# Runtime CLI

The CLI composes the runtime engine, project loader, WXML parser and V8 adapter for repeatable local diagnostics.

```powershell
cargo run -p sdkwork-miniprogram-runtime-cli -- inspect examples/hello-mini-program
cargo run -p sdkwork-miniprogram-runtime-cli -- parse-wxml examples/hello-mini-program/pages/index/index.wxml
cargo run -p sdkwork-miniprogram-runtime-cli -- eval-js "1 + 2"
```

`inspect` validates the `app.json` page tree, resolves source files within the project root, registers app and page scripts in isolated persistent V8, invokes available `onLaunch`, `onShow` and `onLoad` callbacks, returns registration/data snapshots, parses page WXML and reports source-located WXSS subset diagnostics and computed declarations. Event dispatch, full data binding, layout, pixels, `wx` APIs and module loading are not provided.
