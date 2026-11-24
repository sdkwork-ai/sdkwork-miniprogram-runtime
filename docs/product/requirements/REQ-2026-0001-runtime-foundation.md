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
| Cargo workspace, formatting, typed errors, basic lifecycle | In Progress | Workspace build, unit tests, doctests and stable crate contracts |
| Runtime configuration and instance isolation | In Progress | Invalid configurations rejected and per-app state separation tests |
| Standard source project inspection and diagnostics | In Progress | Path containment, manifest/page validation and malformed fixture suite |
| Secure ZIP project packages | In Progress | Bounded path-safe extraction, traversal rejection and entry/expansion limit tests |
| JavaScript execution with V8 | In Progress | One-shot and persistent per-app isolated contexts, configurable source-size and heap limits, compile/runtime diagnostics and host-enforced execution deadlines are tested; CommonJS/ESM, wx APIs and platform distribution remain open |
| CommonJS, ESM, npm resolution and source maps | Planned | Versioned module compatibility contract and resolver/compiler tests |
| WXML parser, AST and source spans | In Progress | Structural AST, nested elements, attributes, text, interpolation tokens, event binding names, source-located `wx:if`/`wx:elif`/`wx:else` and `wx:for` syntax diagnostics are tested; expression and branch evaluation remain planned |
| WXML full expression, loops, conditionals, templates, imports, components, event model | Planned | Versioned grammar and semantic compatibility matrix |
| WXSS structural subset | In Progress | Type/class/ID selectors, descendant matching, specificity/source-order cascade and diagnostics are tested; layout, rpx calculation, page scoping and media behavior remain planned |
| WXS isolation and module behavior | Planned | Dedicated syntax/boundary suite against target docs |
| App/Page/Component lifecycle, observers, behaviors and relations | Planned | Ordered lifecycle traces and component behavior suites |
| Base components and accessibility metadata | Planned | Component-by-component properties/events/platform evidence |
| wx API registry and provider routing | Planned | API schemas, callback/Promise/error tests and provider declarations |
| Storage, virtual file system, network and permissions | Planned | Namespace, path-containment, quota, redaction and host-policy tests |
| Page stack, routing and tab bar | In Progress | Registered-route `navigate_to`, `redirect_to`, `navigate_back`, lifecycle gating and configured depth are tested; tab operations, query parameters, lifecycle callbacks and visual synchronization remain planned |
| WebView and Headless renderers | In Progress | Headless structural Render Tree preserves stable preorder ids, source spans and unevaluated bindings, and exposes a structural WXSS subset; WebView/pixel layout remain planned |
| Android, iOS, Windows, macOS, Linux and Web adapters | Planned | Toolchain, host integration, capability and release evidence per platform |
| CDP gateway, V8 Inspector and custom domains | Planned | Protocol version and method coverage, target discovery and authorization tests |
| SDKWORK DevTools | Planned | Separate React/TypeScript frontend and protocol-only integration tests |
| Rust SDK and C ABI | Planned | ABI ownership, versions, thread/callback and lifecycle conformance tests |
| WASM plugins and trusted native providers | Planned | Permission, signature, API version, quota and isolation tests |
| Performance, memory, resilience and security assurance | Planned | Repeatable baselines, long-run traces and threat-model-based verification |
