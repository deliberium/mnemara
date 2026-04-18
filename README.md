# Mnemara

<p align="center">
  <img src="assets/mnemaraLogo.png" alt="Mnemara logo" width="240">
</p>

<p align="center">
  <a href="https://github.com/deliberium/mnemara/actions/workflows/ci.yml">
    <img src="https://github.com/deliberium/mnemara/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="LICENSE.md">
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

Future work beyond the current release remains in [ROADMAP.md](ROADMAP.md).

## Quick Start

Embedded library usage and daemon-mode deployment are documented here:

- [User Guide](docs/user-guide.md)
- [Architecture](docs/architecture.md)
- [Deployment](docs/deployment.md)
- [JavaScript SDK](sdk/javascript/README.md)
- [Roadmap](ROADMAP.md)
- [Benchmark Methodology](docs/benchmark-methodology.md)
- [Benchmark Results](docs/benchmark-results.md)
- [Ranking Defaults ADR](docs/decision-records/0001-ranking-defaults.md)
- [Security Policy](SECURITY.md)
- [Contributors](CONTRIBUTORS.md)

Run the daemon locally with:

```bash
cargo run -p mnemara-server
```

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

If you want to contribute, please read [CONTRIBUTORS.md](CONTRIBUTORS.md) for the current contribution areas, project priorities, and release-scope guidance.
