---
id: REQ-2026-0001
title: Establish the mini-program runtime foundation and trace product capabilities
owner: SDKWORK Runtime maintainers
status: in-progress
source: platform
problem: The repository has no runtime implementation or repeatable compatibility verification baseline for the approved mini-program runtime product.
goals:
  - Establish a buildable Rust workspace with responsibility-specific crate boundaries.
  - Implement a runnable, diagnosable runtime foundation and a safe source-project loading path.
  - Track every PRD capability with an explicit evidence-based delivery status.
non_goals:
  - Claiming full compatibility or production support for platforms without implementation and verification evidence.
  - Fabricating provider-backed API success or parsing protected package formats.
users:
  - mini-program developer
  - host application developer
  - runtime maintainer
acceptance_criteria:
  - Cargo builds and tests every workspace crate on the declared development target.
  - Runtime configuration failures and package source diagnostics are structured and attributable.
  - Applications in one runtime have independent identifiers and private state namespaces.
  - WXML compiler diagnostics carry source locations for every supported construct and malformed construct.
  - The CLI can inspect a standard source project and report unsupported runtime capabilities explicitly.
  - On Windows, `sdkwork-miniprogram-runtime-cli run <project-dir>` opens the loopback editor workbench with a phone-first preview, selectable tablet and PC viewports, a project Explorer, Monaco source editor, project switching, and a read-only render Inspector with node source locations. The device viewport has no visible scrollbar at the default page size. Saving a supported page source refreshes the preview. The editor server binds to loopback, requires same-origin access, restricts writes to existing project files, and the browser does not execute project JavaScript.
  - Every product capability appears in the capability matrix with an honest delivery state and verification reference.
  - Baseline, naming, manifest, formatting, workspace tests and available security checks are recorded before completion.
non_functional_requirements:
  security: Source paths and archive entries are contained to their application root, resource limits are checked, and unimplemented host capabilities do not return fabricated success.
  privacy: Tokens, credentials, and personal data do not enter logs or fixtures.
  performance: No numerical claim without a reproducible target, fixture, hardware and build profile.
  reliability: Operations use explicit state transitions and structured failures; an app failure must not mutate another app's private state.
affected_surfaces:
  - runtime
  - sdk
  - tests
trace:
  specs:
    - ../sdkwork-specs/REQUIREMENTS_SPEC.md
    - ../sdkwork-specs/RUST_CODE_SPEC.md
    - ../sdkwork-specs/DEPENDENCY_MANAGEMENT_SPEC.md
    - ../sdkwork-specs/SECURITY_SPEC.md
  components:
    - crates/sdkwork-miniprogram-execution-engine
    - crates/sdkwork-miniprogram-package-loader
    - crates/sdkwork-miniprogram-v8-adapter
    - crates/sdkwork-miniprogram-wxml-compiler
    - crates/sdkwork-miniprogram-headless-renderer
    - crates/sdkwork-miniprogram-runtime-cli
verification:
  - cargo fmt --all --check
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo run -p sdkwork-miniprogram-runtime-cli -- --help
  - node ../sdkwork-specs/tools/audit-repository-baseline.mjs --root .
  - node ../sdkwork-specs/tools/check-rust-crate-naming-standard.mjs --root .
  - node ../sdkwork-specs/tools/check-rust-manifest-standard.mjs --root .
---

# Delivery slices

The request covers the full PRD. This first traceable engineering slice establishes requirements and workspace controls while crates implement the initial executable runtime and source processing foundations. Remaining capabilities are separately gated by compatibility, platform, provider, security and DevTools decisions.

## Capability delivery matrix

The PRD defines the target, while this matrix records actual evidence. `Planned`, `In Progress`, `Implemented`, `Tested`, `Verified`, `Unsupported`, `Platform Limited` and `Requires Provider` follow the PRD capability-state vocabulary. A code merge alone does not promote a feature to `Verified`.

| Capability | Initial status | Evidence or next acceptance boundary |
| --- | --- | --- |
| Cargo workspace, formatting, typed errors, basic lifecycle | Tested | Workspace build, strict Clippy, unit tests, doctests, SDKWORK baseline, naming, manifest and component-port checks on Windows x86_64 MSVC |
| Runtime configuration and instance isolation | Tested | Invalid configuration, app storage namespace and persistent V8 context separation tests |
| Standard source project inspection and diagnostics | Tested | Example CLI acceptance test covers project loading, script registration, basic lifecycle callbacks, WXML structure and WXSS subset output |
| Windows run and preview workbench | Tested | CLI integration test starts the `--no-launch` loopback server, verifies editor shell, device controls, read-only Inspector payload and source spans, project discovery/switching, reads and saves page JavaScript, verifies the next preview reflects edited data, and rejects traversal and cross-origin requests. Windows app-window startup and rendered page HTTP response are checked manually. |
| Secure ZIP project packages | Tested | Bounded path-safe extraction, traversal rejection and entry/expansion limit tests on Windows x86_64 |
| JavaScript execution with V8 | In Progress | One-shot and persistent per-app isolated contexts, configurable source-size and heap limits, source-attributed registration errors, compile/runtime diagnostics and host-enforced execution deadlines are tested; CommonJS/ESM, wx APIs and platform distribution remain open |
| Runtime Inspector debug events | Partial | Runtime-owned bounded per-app event journal captures real JavaScript console calls and lifecycle transitions, supports sequence cursors and clear, and accepts redacted network events from a host request provider. CDP gateway/target frontend and a real `wx.request` provider remain open. |
| CommonJS, ESM, npm resolution and source maps | Planned | Versioned module compatibility contract and resolver/compiler tests |
| WXML parser, AST and source spans | Tested | Structural AST, nested elements, attributes, text, interpolation tokens, event binding names, and source-located conditional/loop directives are tested |
| WXML data-aware rendering subset | In Progress | Dotted property bindings, JSON/basic string literals, unary `!`, equality, conditional chains and bounded array loops are tested; JavaScript expressions, templates, imports, component/event semantics remain open |
| WXSS structural subset | Tested | Type/class/ID selectors, descendant matching, specificity/source-order cascade and unsupported syntax diagnostics are tested; layout, rpx calculation, page scoping and media behavior remain open |
| WXS isolation and module behavior | Planned | Dedicated syntax/boundary suite against target docs |
| App/Page registration and basic lifecycle callbacks | In Progress | Isolated app/page script registration, basic `setData`, `onLaunch`, `onShow`, `onLoad` and page data snapshot are tested; ordered full lifecycle, observers, behaviors and component relations remain planned |
| Base components and accessibility metadata | Planned | Component-by-component properties/events/platform evidence |
| wx API registry and provider routing | Planned | API schemas, callback/Promise/error tests and provider declarations |
| Storage and virtual file system | Tested | Per-app in-memory namespaces, path containment, file-count/file-size/aggregate-byte quotas, and close-on-unload are tested; persistence and host integration remain open |
| Network and permissions | Planned | Provider contracts, permission decisions, redirect policy, redaction and host-policy tests |
| Page stack, routing and tab bar | In Progress | Registered-route `navigate_to`, `redirect_to`, `navigate_back`, lifecycle gating and configured depth are tested; tab operations, query parameters, lifecycle callbacks and visual synchronization remain planned |
| WebView and Headless renderers | In Progress | Headless renderer preserves source spans and evaluates the supported page-data subset with structural WXSS matches. The Windows workbench presents that subset in a device-sized viewport. Interactive WebView lifecycle updates, layout, geometry and pixels remain open |
| Android, iOS, Windows, macOS, Linux and Web adapters | Planned | Toolchain, host integration, capability and release evidence per platform |
| CDP gateway, V8 Inspector and custom domains | Partial | The Runtime workbench exposes loopback `/json/version` and `/json/list` discovery and a WebSocket target forwarding to the worker-owned V8 session. Integration tests verify `Runtime.enable` and `Runtime.evaluate`. Debugger pause loop, source mapping, custom domains, authorization tokens, independent SDKWORK DevTools integration and a real Network provider remain required. |
| SDKWORK DevTools | Planned | Separate React/TypeScript frontend and protocol-only integration tests |
| Rust SDK and C ABI | Planned | ABI ownership, versions, thread/callback and lifecycle conformance tests |
| WASM plugins and trusted native providers | Planned | Permission, signature, API version, quota and isolation tests |
| Performance, memory, resilience and security assurance | Planned | Repeatable baselines, long-run traces and threat-model-based verification |
