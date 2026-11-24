# Runtime CLI

The CLI composes the runtime engine, project loader, WXML parser and V8 adapter for repeatable local diagnostics.

```powershell
cargo run -p sdkwork-miniprogram-runtime-cli -- inspect examples/hello-mini-program
cargo run -p sdkwork-miniprogram-runtime-cli -- run examples/hello-mini-program
cargo run -p sdkwork-miniprogram-runtime-cli -- parse-wxml examples/hello-mini-program/pages/index/index.wxml
cargo run -p sdkwork-miniprogram-runtime-cli -- eval-js "1 + 2"
```

`inspect` validates the `app.json` page tree, resolves source files within the project root, registers app and page scripts in isolated persistent V8, invokes available `onLaunch`, `onShow` and `onLoad` callbacks, returns registration/data snapshots, parses page WXML and reports source-located WXSS subset diagnostics and computed declarations. Event dispatch, full data binding, layout, pixels, `wx` APIs and module loading are not provided.

`run` starts a loopback-only editor and preview workbench and opens it in an Edge or Chrome app window on Windows. The left pane simulates phone, tablet and PC viewport sizes. The device viewport is fixed and its scrollbar is hidden. Long page content remains scrollable inside that viewport. The Explorer lists project files, and Monaco Editor supports source editing and saving. Saving a supported page source refreshes the preview. Switch project opens validated sibling projects or accepts another project folder path.

The workbench creates one persistent application Runtime per project and exposes its Runtime-owned CDP target through `/json/version`, `/json/list` and a loopback WebSocket endpoint. `Runtime.enable` and `Runtime.evaluate` are forwarded to the V8 inspector session on its owning worker thread. The in-workbench Inspector displays the rendered Elements tree, computed WXSS subset, evaluated page data, Runtime Console records and renderer diagnostics. Selecting an element highlights it in the preview and shows its WXML source location. Network remains empty until a host-backed `wx.request` provider emits real request lifecycle records. Debugger pause/resume, source mapping, custom MiniProgram domains, authorization tokens and the independent SDKWORK DevTools frontend remain open capabilities. The local HTTP server binds to `127.0.0.1` on an ephemeral port, requires same-origin requests for mutations, and restricts edits to existing UTF-8 files under the selected project root.
