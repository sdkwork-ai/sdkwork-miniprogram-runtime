repository-kind: foundation-dependency

# SDKWORK Mini Program Runtime

Rust implementation of an embeddable mini-program runtime. The product direction and acceptance baseline are in [the PRD](docs/product/prd/PRD.md). Current architecture is described in [the technical architecture](docs/architecture/tech/TECH_ARCHITECTURE.md), while crate ownership, build and test commands are maintained in Cargo and the crate-local component contracts.

## Active project directories

- `crates/` contains Rust libraries and the development CLI.
- `examples/` contains runnable sample projects used to exercise loading and rendering.
- `docs/` contains product requirements, architecture, integration guides and verification evidence.
- `specs/` contains repository-wide machine-readable contracts.
- `tests/` contains cross-crate compatibility fixtures and acceptance tests.

Reserved application, API, SDK, deployment and host-adapter directories remain absent until the runtime owns those deliverables.

## Build and verification

Use Cargo as the Rust workspace and dependency authority.

```powershell
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p sdkwork-miniprogram-runtime-cli -- --help
```

V8 is downloaded and compiled through its published Rust crate distribution at build time. The availability and distribution policy for each production target remains an architecture decision.

## Standards

See the repository [agent entrypoint](AGENTS.md), [component contracts](crates/README.md), and the global SDKWORK Rust, dependency, security and repository standards linked by those contracts.
