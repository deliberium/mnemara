# Mnemara

<p align="center">
  <img src="assets/mnemaraLogo.png" alt="Mnemara logo" width="240">
</p>

<p align="center">
  <a href="https://github.com/deliberium/mnemara/actions/workflows/ci.yml">
    <img src="https://github.com/deliberium/mnemara/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/deliberium/mnemara/blob/master/LICENSE.md">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT">
  </a>
</p>

Mnemara is a local-first, explainable AI memory engine for embedded Rust applications and service-based deployments.

## What It Provides

- product-neutral memory domain model and store traits
- embedded sled-backed storage
- protobuf/gRPC protocol surface
- tonic-based daemon mode
- HTTP/JSON memory, health, and admin endpoints for daemon operations
- reference JavaScript HTTP SDK for non-Rust consumers
- explicit memory scope, trust, and quality state concepts
- retry-safe idempotent writes, batch upserts, and tombstone or hard delete flows
- explainable recall filters plus duplicate-aware compaction, stats, integrity, and repair reporting
- compaction rollup summaries and optional cold-tier archival for stale low-importance records
- configurable recall scorer kinds and scoring profiles across the embedded and daemon-backed stores
- a public semantic embedding seam with a deterministic local reference embedder for integration tests and offline deployments
- opt-in retention enforcement for TTL, archival windows, and namespace caps
- daemon-side request limits for body size, batch breadth, recall breadth, and payload size
- basic daemon metrics export for HTTP and gRPC request activity
- bounded admission control with tenant-aware fairness and runtime status visibility
- public trace listing and lookup APIs with correlation IDs and recall explanations
- portable export/import packages that round-trip across file and sled backends
- gRPC deployment presets for TCP, Unix domain sockets, TLS, and mTLS
- published benchmark methodology, benchmark artifacts, and ranking defaults backed by standard IR metrics

## Release State

The current release includes:

- native Rust embedding through direct crate dependencies
- a local gRPC daemon backed by the sled store
- typed memory records, recall filters, explanations, and planning traces
- compaction, deletion, snapshot, stats, integrity-check, repair, export, and import operations
- published evaluation assets covering ranking quality, backend parity, and portability scenarios

Future work beyond the current release remains in [ROADMAP.md](https://github.com/deliberium/mnemara/blob/master/ROADMAP.md).

## Quick Start

Embedded library usage and daemon-mode deployment are documented here:

- [User Guide](https://github.com/deliberium/mnemara/blob/master/docs/user-guide.md)
- [Architecture](https://github.com/deliberium/mnemara/blob/master/docs/architecture.md)
- [Deployment](https://github.com/deliberium/mnemara/blob/master/docs/deployment.md)
- [JavaScript SDK](https://github.com/deliberium/mnemara/blob/master/sdk/javascript/README.md)
- [Roadmap](https://github.com/deliberium/mnemara/blob/master/ROADMAP.md)
- [Changelog](https://github.com/deliberium/mnemara/blob/master/CHANGELOG.md)
- [Benchmark Methodology](https://github.com/deliberium/mnemara/blob/master/docs/benchmark-methodology.md)
- [Benchmark Results](https://github.com/deliberium/mnemara/blob/master/docs/benchmark-results.md)
- [Ranking Defaults ADR](https://github.com/deliberium/mnemara/blob/master/docs/decision-records/0001-ranking-defaults.md)
- [Security Policy](https://github.com/deliberium/mnemara/blob/master/SECURITY.md)
- [Contributors](https://github.com/deliberium/mnemara/blob/master/CONTRIBUTORS.md)

Run the daemon locally with:

```bash
cargo run -p mnemara-server
```

## Installation

Mnemara supports two installation paths:

- develop from a checked-out source workspace
- consume published crates from crates.io

### From a source checkout

Clone the repository and build the workspace:

```bash
git clone https://github.com/deliberium/mnemara.git
cd mnemara
cargo build --workspace
```

Run the full test suite with:

```bash
cargo test --workspace
```

Run the daemon from the checked-out workspace with:

```bash
cargo run -p mnemara-server
```

If you want to depend on the facade crate or a backend crate directly from a local checkout, use a path dependency:

```toml
[dependencies]
mnemara = { path = "crates/mnemara", features = ["sled"] }
```

### From published crates

For embedded library usage, add the facade crate to your application:

```bash
cargo add mnemara --features sled
```

You can swap `sled` for `file`, `protocol`, or `server`, or enable multiple features as needed.

If you want individual crates instead of the facade, add them directly:

```bash
cargo add mnemara-core
cargo add mnemara-store-sled
```

For the daemon binary, use `cargo install` against the published server crate:

```bash
cargo install mnemara-server
```

Then run it with:

```bash
mnemara-server
```

`cargo install` is only for binary crates such as `mnemara-server`. The facade crate `mnemara` is a library crate, so applications should consume it with `cargo add` or a `Cargo.toml` dependency entry instead.

## Publishing

If you plan to publish the workspace crates to crates.io, publish them in dependency order:

1. `mnemara-core`
2. `mnemara-store-file`, `mnemara-store-sled`, and `mnemara-protocol`
3. `mnemara-server`
4. `mnemara`

The order matters because `cargo package` and `cargo publish` resolve internal path dependencies through crates.io during verification. Before `mnemara-core` is published, crates such as `mnemara-store-file`, `mnemara-store-sled`, `mnemara-server`, and the facade crate `mnemara` cannot complete a publish verification run.

Recommended release checks:

```bash
./scripts/release-checklist.sh preflight
./scripts/release-checklist.sh foundation
./scripts/release-checklist.sh dry-run-publish foundation
```

After publishing the lower-level crates, repeat `cargo package` or `cargo publish --dry-run` for the remaining crates in order. The checklist script lives at [scripts/release-checklist.sh](https://github.com/deliberium/mnemara/blob/master/scripts/release-checklist.sh), and the crate README recommendation audit lives at [docs/crates-io-readme-audit.md](https://github.com/deliberium/mnemara/blob/master/docs/crates-io-readme-audit.md).

## Workspace Layout

- `crates/mnemara`: facade crate that re-exports core types and opt-in file, sled, protocol, and server surfaces
- `crates/mnemara-core`: product-neutral domain model and store traits
- `crates/mnemara-store-file`: compatibility-oriented file store
- `crates/mnemara-store-sled`: embedded sled-backed store
- `crates/mnemara-protocol`: protobuf/gRPC schema package
- `crates/mnemara-server`: tonic-based daemon implementation
- `sdk/javascript`: reference JavaScript SDK over the HTTP API

## Facade Crate

Applications can depend on `mnemara` and enable only the product surfaces they need:

```toml
[dependencies]
mnemara = { version = "0.1.0", features = ["sled"] }
```

Available facade features:

- `file`: re-export `mnemara-store-file`
- `sled`: re-export `mnemara-store-sled`
- `protocol`: re-export `mnemara-protocol`
- `server`: re-export `mnemara-server` and its protocol/sled dependencies
- `all`: enable every facade feature

## Design Principles

- local-first by default
- explainable retrieval over opaque ranking
- explicit memory classes rather than transcript blobs only
- stable namespace and tenant isolation
- support for both embedded and service-based deployment modes

## Project Status

Mnemara now ships the extracted core/store/protocol/server workspace, the facade crate, a reference JavaScript HTTP SDK, published benchmark artifacts, portable import/export workflows, bounded admission control, public trace APIs, and validated TCP/UDS/TLS/mTLS daemon deployment modes.

## Open Source and Contributions

Mnemara is an open source project, and contributions are welcome.

If you want to contribute, please read [CONTRIBUTORS.md](https://github.com/deliberium/mnemara/blob/master/CONTRIBUTORS.md) for the current contribution areas, project priorities, and release-scope guidance.
