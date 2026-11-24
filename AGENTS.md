# Repository agent entrypoint

This repository is a Rust mini-program runtime foundation. Read only the task-relevant global specifications in the sibling `../sdkwork-specs/` checkout.

## Task routing

- Rust code, Cargo manifests and tests use `../sdkwork-specs/RUST_CODE_SPEC.md` and `../sdkwork-specs/DEPENDENCY_MANAGEMENT_SPEC.md`.
- Product requirements and acceptance use `docs/product/prd/PRD.md` and `docs/product/requirements/REQ-2026-0001-runtime-foundation.md`, with `../sdkwork-specs/REQUIREMENTS_SPEC.md`.
- Runtime boundaries and security use `../sdkwork-specs/SECURITY_SPEC.md`, and when changed `../sdkwork-specs/PRIVACY_SPEC.md`.
- Rust crate contracts live in each `crates/*/specs/component.spec.json`.

## Verification

Run `cargo fmt --all --check`, targeted crate tests, then `cargo test --workspace`. Run the Rust manifest and naming checks from `../sdkwork-specs/` after changing Cargo manifests. Run the repository baseline audit there before claiming baseline compliance.

Do not claim PRD capabilities as implemented unless their acceptance tests and platform evidence verify them. Keep unsupported capabilities explicit and return structured errors instead of synthetic success.
