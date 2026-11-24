# SDKWORK Mini Program Runtime PRD

Status: active product baseline
Version: 1.0.0
Source: `sdkwork-miniprogram-runtime-PRD.md` supplied by the product owner
Specs: `../sdkwork-specs/REQUIREMENTS_SPEC.md`, `../sdkwork-specs/TEST_SPEC.md`

## Product definition

An embeddable, cross-platform mini-program runtime implemented primarily in Rust. It loads and executes supported mini-program projects, provides compatible rendering and host APIs, isolates applications, and exposes inspection and debugging through CDP and SDKWORK DevTools.

## Goals

- Provide an independently embeddable Rust runtime core and Rust/C ABI SDK surfaces.
- Load standard source projects and a documented runtime package format.
- Implement public mini-program language, lifecycle, component, routing and API behavior against a versioned compatibility profile.
- Support WebView and Headless renderers first, with future Native and Web renderers.
- Provide authenticated debugging, inspection, network and performance tooling.
- Maintain compatibility evidence by capability, platform and target specification version.

## Non-goals

- Claiming official WeChat certification or access to private services or APIs.
- Bypassing host permissions, protected service requirements or proprietary package protections.
- Claiming full compatibility without a complete tested compatibility matrix.
- Delivering a native renderer or complete production platform binding before its platform contracts are established.

## Acceptance baseline

Each release declares runtime and package format versions, the compatibility profile, supported platforms, renderer support and tested capabilities. Source loading reports path-specific diagnostics. Application contexts isolate their execution, storage, files, permissions and debug access. API capabilities without a real provider report `Unsupported` or `Requires Provider`. CDP targets require explicit authorization and default to localhost in debug builds. Every verified behavior is backed by a reproducible test suite and environment record.

## Product scope and delivery status

The complete requested product scope is maintained in [the capability matrix](../requirements/REQ-2026-0001-runtime-foundation.md#capability-delivery-matrix). Initial engineering starts with Cargo workspace and runtime foundations. The PRD's M1-M5 language, compatibility, renderer, inspection, platform, C ABI, security and DevTools work remains unverified until corresponding implementation and evidence exist.

## Open product and architecture inputs

- Supported public compatibility documentation and target baseline-library versions.
- V8 distribution, build, licensing and validation per platform, including iOS constraints.
- Stable source/package manifest, package size limits and optional signature policy.
- CDP version and supported method matrix.
- Provider contracts, host permissions and production process-isolation requirements.
- Criteria for promotion of a platform or capability from implemented to tested and verified.
