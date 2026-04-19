# Mnemara User Guide

## Installation

Mnemara can be used either as an embedded Rust library dependency or as a local gRPC or HTTP daemon.

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
- effective planning profile
- effective policy profile
- selected scoring channels
- per-hit score breakdowns, including metadata, episodic, salience, and curation signals
- a planning trace with selection rank, planner stages, candidate sources, selected channels, and filter reasons
- correlation IDs that tie recall output back to daemon request traces

## Episodic and continuity-aware recall

Records can now carry optional episodic context so recall can reason about more
than flat text matches.

Shipped episodic fields include:

- `episode_id`
- continuity state such as open, resolved, superseded, or abandoned
- causal, previous, next, and related record links
- salience signals such as reuse, novelty, goal relevance, and unresolved weight
- optional affective annotations with explicit provenance

Useful episodic query patterns are:

- set `filters.episode_id` when you already know the active thread
- set `filters.unresolved_only = true` when you want open loops and unfinished work
- set `filters.temporal_order` to chronological order when sequence matters more than pure relevance

Continuity-sensitive queries such as “what led to this,” “what changed,” or
“what happened next” can trigger the continuity-aware planner profile and
enrich the resulting planning trace.

## Ranking and evaluation

Ranking is controlled through:

- `EngineConfig.recall_scorer_kind`
- `EngineConfig.recall_scoring_profile`
- `EngineConfig.recall_planning_profile`
- `EngineConfig.recall_policy_profile`
- `EngineConfig.graph_expansion_max_hops`
- `EngineConfig.embedding_provider_kind`

Available scorer families:

- `Profile` for profile-weighted ranking
- `Curated` for stronger trust/quality-state promotion

Available profiles:

- `Balanced`
- `LexicalFirst`
- `ImportanceFirst`

Available planning profiles:

- `FastPath`
- `ContinuityAware`

Available policy profiles:

- `General`
- `Support`
- `Research`
- `Assistant`
- `AutonomousAgent`

`FastPath` is the low-latency default. `ContinuityAware` enables bounded
continuity expansion, provenance-aware overlay behavior, and richer candidate
trace data for episode-sensitive workloads.

`graph_expansion_max_hops` is the hard cap on continuity expansion depth.
Current bounded relation families include same-episode membership, chronology
links, causal links, related-record links, and lineage references. Queries stay
within the request scope while using those relations.

`General` is the default workload profile. `Support` pushes ranking toward
current, verified, and pinned facts. `Research` is more tolerant of archival
and derived context when broad recall matters. `Assistant` keeps a balanced
continuity posture for conversational assistance. `AutonomousAgent` applies a
stricter provenance bias for task-execution workflows where low-trust context
should be discounted more aggressively.

Embedding providers remain optional. `Disabled` is the safe fallback and keeps
recall lexical/metadata/episodic only. `DeterministicLocal` enables the shipped
semantic channel without requiring a hosted dependency.

For Rust callers that need a custom semantic provider without changing the core
engine config enum, the supported extension seam is the shared-embedder path:

```rust
use std::sync::Arc;

use mnemara_core::{
  RecallPlanner, RecallPlanningProfile, RecallPolicyProfile, RecallScorerKind,
  RecallScoringProfile, SemanticEmbedder,
};

let planner = RecallPlanner::with_shared_embedder(
  RecallPlanningProfile::ContinuityAware,
  1,
  RecallScorerKind::Profile,
  RecallScoringProfile::Balanced,
  RecallPolicyProfile::General,
  Arc::new(my_embedder),
  "embedding_provider=my_custom_provider",
);
```

The final string becomes part of explanation policy notes, so the active custom
provider remains inspectable in traces and downstream tooling.

## Historical and lifecycle-aware recall

Mnemara now separates lifecycle visibility into two axes:

- `quality_state` describes whether the record is active, archived, suppressed, or deleted
- `historical_state` describes whether the record is current, historical, or superseded

This matters during recall. Default recall is intentionally current-oriented.
If you want archival or superseded context after compaction or retention work,
set `filters.historical_mode` explicitly.

Available historical modes are:

- `CurrentOnly`
- `IncludeHistorical`
- `HistoricalOnly`

Use `filters.lineage_record_id` when you want to inspect a summary or
superseded record together with related lineage-connected records.

Compaction and retention can now preserve provenance by creating summary
records, marking stale records historical, and marking duplicate rollups as
superseded instead of silently hiding the maintenance history.

The current shipped contradiction and drift rules are intentionally conservative:

- default recall prefers current records over historical or superseded ones
- `HistoricalOnly` can intentionally elevate historical and superseded context
- lineage links such as `ConflictsWith`, `SupersededBy`, and `ConsolidatedFrom`
  preserve disagreement and derivation instead of collapsing them into one
  claimed truth
- operators should use `lineage_record_id` plus historical recall when they want
  to inspect contradictory or drifted state explicitly

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

## Episodic Protocol Contract

The episodic API and protocol surface is additive across embedded Rust,
HTTP/JSON, gRPC/protobuf, and portable import/export packages.

For record payloads, these episodic and lifecycle fields are additive:

- `episode`
- `historical_state`
- `lineage`

Inside `episode`, `schema_version` is explicit and currently defaults to `1`
when omitted by older clients or legacy portable packages.

Inside `episode`, the shipped additive fields include:

- `schema_version`
- `episode_id`
- `summary`
- `continuity_state`
- `actor_ids`
- `goal`
- `outcome`
- `started_at_unix_ms`
- `ended_at_unix_ms`
- `last_active_unix_ms`
- `recurrence_key`
- `recurrence_interval_ms`
- `boundary_label`
- `previous_record_id`
- `next_record_id`
- `causal_record_ids`
- `related_record_ids`
- `linked_artifact_uris`
- `salience`
- `affective`

For recall requests, the additive episodic and lifecycle query controls are:

- `episode_id`
- `continuity_states`
- `unresolved_only`
- `temporal_order`
- `historical_mode`
- `lineage_record_id`

Compatibility rules for clients and packages are intentionally conservative:

- clients may omit all episodic fields and keep baseline non-episodic behavior
- missing `episode.schema_version` defaults to `1`
- missing additive episodic fields deserialize to safe defaults
- missing `episode` stays `None`
- missing `historical_state` defaults to `Current`
- missing `lineage` defaults to an empty list
- unknown future additive fields in JSON packages are ignored by current import code

When you attach a record to an episode, current write validation enforces these
association rules:

- `episode_id` must be non-empty
- `schema_version` must match the current supported episode contract
- `ended_at_unix_ms` cannot be earlier than `started_at_unix_ms`
- `last_active_unix_ms` must stay within the episode timeline when both bounds are present
- `recurrence_interval_ms`, when present, must be greater than zero and paired with `recurrence_key`
- `boundary_label`, when present, must be non-empty
- `actor_ids`, when provided, must include the record's owning `scope.actor_id`
- `previous_record_id`, `next_record_id`, `causal_record_ids`, and `related_record_ids` cannot self-reference the current record

For derived affective annotations, write validation also requires:

- `urgency`, `confidence`, and `tension` to stay within `0.0..=1.0`
- `tone` and `sentiment` to be non-empty when present
- `provenance=Derived` to keep `confidence < 1.0` so derived signals cannot be stored as certain facts

That means older callers can keep sending record-only payloads, while newer
callers can add episodic and lifecycle fields incrementally without a breaking
schema migration.

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
