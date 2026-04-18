# mnemara-core

`mnemara-core` provides the product-neutral memory domain model, query types, scoring configuration, evaluation helpers, and async store traits that the rest of the Mnemara workspace builds on.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-core
```

## What it contains

- `MemoryRecord`, `MemoryScope`, `MemoryRecordKind`, `MemoryQualityState`, and `MemoryTrustLevel`
- `RecallQuery`, `RecallFilters`, `RecallResult`, and recall explanation types
- `BatchUpsertRequest`, `UpsertRequest`, `DeleteRequest`, and admin operation request and report types
- `EngineConfig`, scoring profiles, embedding-provider configuration, and evaluation helpers
- `MemoryStore`, the async trait implemented by the file and sled backends

## Minimal example

```rust
use mnemara_core::{EngineConfig, RecallQuery, RecallScoringProfile};

let mut config = EngineConfig::default();
config.recall_scoring_profile = RecallScoringProfile::Balanced;

let query = RecallQuery {
    query_text: "reconnect storm mitigation".to_string(),
    ..Default::default()
};
```

## Related crates

- `mnemara` for the facade crate
- `mnemara-store-file` for the file-backed implementation
- `mnemara-store-sled` for the embedded sled-backed implementation
- `mnemara-protocol` for protobuf and gRPC surface types
- `mnemara-server` for the standalone daemon

Project documentation: <https://github.com/deliberium/mnemara>
