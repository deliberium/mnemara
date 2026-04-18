# Mnemara User Guide

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
