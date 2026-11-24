# SDKWORK Mini Program Runtime Technical Architecture

Status: active baseline, product architecture in progress
Owner: SDKWORK Runtime maintainers
Updated: 2026-11-24
Specs: `../sdkwork-specs/ARCHITECTURE_DECISION_SPEC.md`, `../sdkwork-specs/RUST_CODE_SPEC.md`, `../sdkwork-specs/DEPENDENCY_MANAGEMENT_SPEC.md`, `../sdkwork-specs/SECURITY_SPEC.md`

## Product boundary

The repository owns portable Rust runtime libraries, source project/package readers, compatibility subsystems, renderer and host-provider contracts, inspector protocol services, SDK surfaces and compatibility evidence. Platform applications and a standalone React/TypeScript DevTools product remain separately scoped deliverables connected through public contracts.

## Crate direction

The initial crates are organized by responsibility. The execution engine owns runtime configuration, application contexts and lifecycle/storage primitives. The package loader owns project metadata, resource containment and bounded archive extraction. The WXML compiler owns source spans, parsing, diagnostics and a syntax tree. The V8 adapter owns one-shot isolated JavaScript evaluation with explicit source-size limits and a host-enforced execution deadline. The headless renderer maps parsed syntax trees into a stable structural Render Tree while preserving bindings and source spans. The CLI composes these public surfaces for inspection and local validation. The CLI may depend on the libraries, while lower-level compiler, package and renderer crates do not depend on the CLI or engine.

The PRD's long-term target adds independent compatibility, renderer, provider, inspector and ABI responsibilities as their contracts mature. No crate claims whole-product compatibility solely by exposing a similarly named API.

## Execution and isolation

Each loaded application owns an application identifier, execution context, module state, storage namespace, virtual file root, permission and network policies, resource budget and authorized debug session. Runtime handles validate state transitions and release resources on unload. Cross-application data access is unavailable through normal app APIs.

V8 is the initial engine target using the upstream Rust crate distribution. Isolates do not constitute operating-system isolation. Embedding hosts must choose deployment isolation and provider policy appropriate to trust boundaries. V8 distribution, build feasibility and licensing must be verified per platform before support is claimed.

## Renderer and platform boundaries

An engine-facing Render Tree and event contract separates mini-program semantics from Headless and WebView implementations. Native rendering and Web/browser adapters are later platform deliverables. Host APIs are accessed through permission-checked providers. Provider-dependent capabilities report required configuration or explicit unsupported states.

## Inspector and SDK boundaries

CDP and SDKWORK MiniProgram protocol surfaces consume versioned public inspection data. They do not read internal Rust state directly. Rust API is the first embedding surface. C ABI and language bindings require explicit ownership, string encoding, thread and callback lifetime contracts before release.

## Version and verification policy

Runtime, compatibility profile, package format, renderer, platform adapter, CDP and MiniProgram protocol versions are tracked independently. The capability delivery matrix is the index of implementation status and acceptance evidence. Performance and platform claims require reproducible target-specific test records.

## Open decisions

See [ADR index](../decisions/README.md). Initial unresolved choices include target public compatibility versions, V8 distribution on every platform, package manifest and signing, CDP version, process isolation deployment, and provider permission contracts.
