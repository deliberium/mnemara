# Copilot Instructions for Mnemara

## Build, test, and lint

- Build the Rust workspace: `cargo build --workspace`
- Run the full Rust test suite: `cargo test --workspace`
- Run a single server integration test: `cargo test -p mnemara-server --test service_roundtrip upsert_recall_snapshot_and_compact_round_trip -- --exact`
- Check formatting: `cargo fmt --all --check`
- Run lints: `cargo clippy --workspace --all-targets`
- Run the daemon locally: `cargo run -p mnemara-server`
- Validate the JavaScript SDK package shape: `cd sdk/javascript && npm pack --dry-run`

## High-level architecture

- This is a Rust workspace centered on `mnemara-core`, which defines the domain model, query/config types, recall scoring, semantic embedding seam, and the `MemoryStore` trait. Other crates are adapters around those core types.
- `mnemara-store-sled` is the main durable backend used by the daemon. `mnemara-store-file` is a filesystem-backed compatibility backend with parallel behavior and replay-fixture coverage. Both implement the same `MemoryStore` contract and both persist idempotency mappings alongside records.
- `mnemara-protocol` owns the protobuf schema in `proto/mnemara/v1/memory.proto` and generates the gRPC types with a vendored `protoc` in `build.rs`.
- `mnemara-server` wraps a shared `MemoryStore` in two transport surfaces at once: tonic gRPC and an axum HTTP/JSON API. `src/main.rs` is the runtime composition point where environment variables are translated into `EngineConfig`, `ServerLimits`, and `AuthConfig`, then both servers are started against the same `SledMemoryStore`.
- `sdk/javascript` is a thin reference client for the HTTP API only. It mirrors the daemon's HTTP wire format and is the non-Rust integration example in-tree.

## Key conventions

- Preserve the product boundary from `docs/architecture.md`: keep Mnemara memory-centric and push app-specific orchestration, UI concepts, and auth contracts into adapters or companion layers.
- Preserve first-release scope from `README.md` and `CONTRIBUTORS.md`. If a capability is clearly beyond the currently implemented embedded store + daemon + JS SDK surface, treat it as roadmap work rather than implied current behavior.
- Idempotency is a core behavior, not an optional convenience. Upserts carry an `idempotency_key`, and both store backends scope that key by tenant, namespace, actor, conversation, and session. Changes to write paths should preserve retry-safe semantics and the existing deduplication receipts.
- Engine tuning is expected to flow end-to-end. If you add retrieval, scoring, compaction, or embedding knobs, wire them through `mnemara-core::EngineConfig`, the store implementation, daemon env parsing, snapshot/stats payloads, and recall explanations so operators can see the active configuration remotely.
- Keep the HTTP and gRPC surfaces feature-aligned. The server crate exposes the same store operations through both transports, with shared limits, auth, metrics, and admin semantics.
- When changing the API contract, update all exposed layers together: core request/response types, both store backends, `mnemara-protocol` protobuf definitions, server mappings, and the JavaScript SDK if the HTTP surface changes.
- JSON-facing enums follow the daemon's existing wire format used by the JS SDK, for example `Episodic` and `Verified`, while protobuf uses the generated enum values from `mnemara-protocol`.
