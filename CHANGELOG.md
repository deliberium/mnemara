# Changelog

<!-- markdownlint-disable MD024 -->

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project aims to follow Semantic Versioning.

## [Unreleased]

### Added

- additive episodic record fields for continuity state, salience, causal links, and optional affective annotations
- retrieval-planner profiles with planner-stage and candidate-source traces for explainable continuity-aware recall
- lifecycle-aware historical-state and lineage-link model fields across the core, protocol, daemon, and backend layers
- compaction and maintenance counters for superseded records, historical records, and lineage links
- workload-aware retrieval policy profiles for general, support, research, assistant, and autonomous-agent recall tuning
- public shared-embedder constructors for custom semantic providers in the scorer and planner path

### Changed

- recall explanations now expose planning profile, episodic and salience score channels, and richer planner trace detail
- recall explanations and engine tuning exports now disclose the effective retrieval policy profile alongside scorer and planner metadata
- custom semantic provider notes now flow through explanation policy notes on the same path as built-in embedding providers
- compaction and archival flows now preserve current versus historical versus superseded visibility instead of treating all archived data as the same state
- repository docs, deployment guidance, benchmark docs, and website rollout copy were aligned with shipped episodic, planner, and lifecycle behavior
- benchmark report v1 now publishes the expanded 16-case corpus, lifecycle-sensitive scenario tables, and the continuity-aware planner profile slice

### Fixed

- archived replay and roundtrip coverage now explicitly exercises historical recall visibility after lifecycle-aware maintenance transitions
- daemon startup now honors the documented `MNEMARA_RECALL_PLANNING_PROFILE` and `MNEMARA_GRAPH_EXPANSION_MAX_HOPS` fallback controls, with regression coverage for safer rollout posture
- policy-tuned provenance ranking now applies the configured workload profile consistently across file, sled, daemon, and transport explanation surfaces

## [0.1.0] - 2026-04-18

Initial public workspace release.

### Added

- facade crate `mnemara` with feature-gated re-exports for the file, sled, protocol, and server surfaces
- core domain model and async store traits in `mnemara-core`, including scoped memory records, recall queries, explanations, planning traces, and maintenance report types
- embedded file-backed and sled-backed storage implementations with batch upsert, snapshot, delete, compaction, integrity check, repair, export, and import support
- protobuf and gRPC API surface in `mnemara-protocol` for Rust client and server integration
- standalone daemon in `mnemara-server` with tonic gRPC service plus HTTP/JSON admin endpoints
- reference JavaScript HTTP SDK for non-Rust consumers
- explainable recall with per-hit score breakdowns, selected channels, planning traces, and correlation IDs
- configurable recall scorer kinds and scoring profiles for balanced, lexical-first, and importance-first retrieval behavior
- semantic embedding seam with a deterministic local provider for offline usage and integration testing
- ranking evaluation assets and benchmark artifacts documenting quality and backend parity
- retry-safe idempotent writes and batch upserts
- tombstone and hard-delete flows for memory records
- duplicate-aware compaction with rollup summaries and optional cold-tier archival for stale low-importance records
- retention controls for TTL expiry, archival windows, and namespace caps
- backend-neutral export and import packages with validate, merge, replace, and dry-run flows across file and sled stores
- daemon deployment profiles for TCP, Unix domain sockets, TLS, and mutual TLS
- request-size and batch-size guardrails plus bounded admission control with tenant-aware fairness
- metrics export, request traces, runtime status visibility, and trace filtering APIs
- HTTP admin surfaces for health, readiness, snapshot, stats, integrity, repair, compact, delete, trace lookup, runtime status, export, and import operations
- architecture, deployment, user guide, benchmark methodology, benchmark results, ranking defaults ADR, and security policy documentation
- release checklist script and crates.io README audit for publication readiness
- installation and publishing guidance for source checkouts, published crates, and the daemon binary

[Unreleased]: https://github.com/deliberium/mnemara/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/deliberium/mnemara/releases/tag/v0.1.0
