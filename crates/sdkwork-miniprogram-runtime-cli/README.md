# Runtime CLI

The CLI composes the runtime engine, project loader, WXML parser and V8 adapter for repeatable local diagnostics.

```powershell
cargo run -p sdkwork-miniprogram-runtime-cli -- inspect examples/hello-mini-program
cargo run -p sdkwork-miniprogram-runtime-cli -- parse-wxml examples/hello-mini-program/pages/index/index.wxml
cargo run -p sdkwork-miniprogram-runtime-cli -- eval-js "1 + 2"
```

`inspect` validates the `app.json` page tree, resolves source files within the project root, parses each page WXML and exercises the initial application lifecycle. It does not claim to render pages or execute `app.js`/page JavaScript as a persistent application.
