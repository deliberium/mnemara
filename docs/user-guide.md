# Mnemara User Guide

## Installation

Mnemara can be used either as a Rust library dependency or as a standalone daemon.

### Develop from a checked-out repository

Clone the repository, build it, and run tests from the workspace root:

```bash
git clone https://github.com/deliberium/mnemara.git
cd mnemara
cargo build --workspace
cargo test --workspace
```

To run the daemon from source:

```bash
cargo run -p mnemara-server
```

To consume the facade crate from the local checkout in another Rust project, point at the workspace path:

```toml
[dependencies]
mnemara = { path = "../mnemara/crates/mnemara", features = ["sled"] }
```

### Use published crates

For embedded usage, depend on the facade crate and enable only the surfaces you need:

```bash
cargo add mnemara --features sled
```

Common facade feature choices are:

- `file` for the file-backed store
- `sled` for the embedded sled-backed store
- `protocol` for protobuf and gRPC types
- `server` for the daemon surface and its protocol and sled dependencies

If you prefer direct crate dependencies instead of the facade, add them individually:

```bash
cargo add mnemara-core
cargo add mnemara-store-file
cargo add mnemara-store-sled
```

For daemon deployments, install the published binary crate:

```bash
cargo install mnemara-server
```

Then start the service with:

```bash
mnemara-server
```

`cargo install` does not apply to the facade crate `mnemara`, because it is a library crate rather than a binary.

## Publishing the crates

For crates.io publication, release the workspace in dependency order:

1. `mnemara-core`
2. `mnemara-protocol`
3. `mnemara-store-file` and `mnemara-store-sled`
4. `mnemara-server`
5. `mnemara`

This is required because Cargo verifies path dependencies against crates.io during packaging. `mnemara-protocol` is independent of the other workspace crates, but the file and sled backends both depend on `mnemara-core`, `mnemara-server` depends on `mnemara-core`, `mnemara-protocol`, and `mnemara-store-sled`, and the facade crate `mnemara` depends on the full workspace graph through its optional features.

Use `cargo package` or `cargo publish --dry-run` as the final pre-release check for each crate before moving to the next step in the sequence. The scripted version of that flow is in `scripts/release-checklist.sh`, including the staged `dry-run-publish` phase. Its `all` target validates every crate it can and reports the crates that are still blocked by publish-order prerequisites. The crate landing-page recommendation audit is documented in `docs/crates-io-readme-audit.md`.

## What Mnemara is for

Mnemara is a memory engine for applications that need durable, scoped recall instead of application-specific orchestration.

It stores memories by:

- tenant
- namespace
- actor
- conversation and session
- trust level
- quality state
- memory kind

## Writing good memories

High-signal writes are:

- idempotent
- scoped correctly
- labeled with trustworthy provenance
- concise enough to retrieve cheaply
- durable only when they are worth keeping

Retry safety is built around `idempotency_key`, and portable exports preserve those mappings alongside records.

## Querying recall well

Useful recall queries provide:

- the right scope
- a clear query string
- a realistic `max_items` and optional `token_budget`
- `include_explanation=true` when operators or downstream agents need traceability

Recall explanations now include:

- scorer family and scoring profile
- selected scoring channels
- per-hit score breakdowns, including metadata and curation signals
- a planning trace with selection rank, selected channels, and filter reasons
- correlation IDs that tie recall output back to daemon request traces

## Ranking and evaluation

Ranking is controlled through:

- `EngineConfig.recall_scorer_kind`
- `EngineConfig.recall_scoring_profile`
- `EngineConfig.embedding_provider_kind`

Available scorer families:

- `Profile` for profile-weighted ranking
- `Curated` for stronger trust/quality-state promotion

Available profiles:

- `Balanced`
- `LexicalFirst`
- `ImportanceFirst`

The checked-in evaluation corpus and published benchmark artifacts document how those choices behave across exact lookup, duplicate-heavy, recent-thread, durable-high-trust, archival, noisy, portability, fairness, and deployment scenarios.

## Portability workflows

Use `export` to create backend-neutral packages filtered by tenant and namespace.

Use `import` with:

- `Validate` to verify package version and record validity without writing
- `Merge` to keep existing records and skip duplicates by record ID
- `Replace` to clear the target backend before import
- `dry_run=true` to preview the outcome and receive structured failures

Portable import reports now tell you:

- whether the package version is compatible
- how many records validated
- how many would be or were imported
- how many were skipped
- which records failed validation and why

## Daemon observability

The daemon publishes:

- request traces for write, recall, and admin operations
- backend and admission-class metadata on each trace
- trace correlation IDs and planning-trace IDs for recall
- runtime admission status with queue depth and wait telemetry
- trace-retention saturation and eviction counts

Use `/admin/traces` for recent request history and `/admin/runtime` for live fairness/retention state.

## Deployment guidance

- prefer `uds-local` for same-host local agents
- use `tls-service` or `mtls-service` when exposing gRPC over a network
- keep metrics protected in shared environments
- tune per-class inflight and queue settings before raising record-content or batch-size limits

Feature work that remains beyond the current release continues to live in [ROADMAP.md](../ROADMAP.md).
