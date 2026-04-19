# mnemara-core

`mnemara-core` provides the product-neutral memory domain model, query types, scoring configuration, evaluation helpers, portable package types, and async store traits that the rest of the Mnemara workspace builds on.

Choose this crate when you need the typed memory model and retrieval or maintenance contracts, but not a specific storage backend or transport surface.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-core
```

## What it contains

- `MemoryRecord`, `MemoryScope`, `MemoryRecordKind`, `MemoryQualityState`, `MemoryHistoricalState`, and `MemoryTrustLevel`
- episodic context types, continuity state, salience, recurrence, duration, boundary cues, and lineage links
- `RecallQuery`, `RecallFilters`, `RecallResult`, recall explanations, and continuity-aware planning trace types
- `BatchUpsertRequest`, `UpsertRequest`, `DeleteRequest`, `ArchiveRequest`, `SuppressRequest`, `RecoverRequest`, and admin operation reports
- `SnapshotManifest`, portable export or import package types, integrity or repair reports, and maintenance stats
- `EngineConfig`, scorer kinds, scoring and policy profiles, planning profiles, embedding-provider configuration, and evaluation helpers
- `MemoryStore`, the async trait implemented by the file and sled backends

## Minimal example

```rust
use mnemara_core::{
    EngineConfig, RecallFilters, RecallHistoricalMode, RecallQuery, RecallScoringProfile,
    RecallTemporalOrder,
};

let mut config = EngineConfig::default();
config.recall_scoring_profile = RecallScoringProfile::Balanced;

let query = RecallQuery {
    query_text: "reconnect storm mitigation".to_string(),
    filters: RecallFilters {
        unresolved_only: true,
        historical_mode: RecallHistoricalMode::IncludeHistorical,
        temporal_order: RecallTemporalOrder::NewestFirst,
        ..Default::default()
    },
    ..Default::default()
};
```

## Notes

- additive schema evolution keeps missing episodic or lifecycle fields backward compatible for existing JSON records and portable packages
- embedded callers can keep record-only usage, or opt into episodic recall, planner traces, lineage-aware retrieval, and lifecycle controls as needed
- custom semantic providers can integrate through the public embedding seam without changing the domain model

## Related crates

- `mnemara` for the facade crate
- `mnemara-store-file` for the file-backed implementation
- `mnemara-store-sled` for the embedded sled-backed implementation
- `mnemara-protocol` for protobuf and gRPC surface types
- `mnemara-server` for the daemon crate and reusable service surfaces

Project documentation: <https://github.com/deliberium/mnemara>
