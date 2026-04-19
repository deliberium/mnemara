# mnemara-protocol

`mnemara-protocol` publishes the protobuf and gRPC types used by the Mnemara service surface.

Choose this crate when you need Rust client or server access to the wire contract without pulling in a storage backend or the full daemon implementation.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-protocol
```

## What it exposes

- generated protobuf messages under `mnemara_protocol::v1`
- generated gRPC client and server traits for the Mnemara memory service
- episodic record fields, lineage links, recall filter controls, and planning-trace payloads
- lifecycle, stats, integrity, trace, and portable import or export RPCs

## Minimal example

```rust
use mnemara_protocol::v1::{RecallFilters, RecallRequest};

let request = RecallRequest {
    query_text: "reconnect storm mitigation".to_string(),
    filters: Some(RecallFilters {
        unresolved_only: true,
        ..Default::default()
    }),
    ..Default::default()
};
```

## Notes

- intended for Rust gRPC clients and servers that need the wire-level schema
- the schema includes additive episodic memory fields such as episode context, recurrence, boundary cues, historical state, and lineage
- recall filters cover historical and continuity-aware retrieval controls such as `episode_id`, `continuity_states`, `unresolved_only`, `temporal_order`, `historical_mode`, and `lineage_record_id`
- lifecycle RPCs cover archive, suppress, recover, stats, integrity, repair, trace lookup, export, and import flows in addition to upsert, recall, compact, snapshot, and delete
- pairs with `mnemara-server` for the reference daemon implementation
- embedded applications that only need the domain model should depend on `mnemara-core` or the `mnemara` facade instead

Project documentation: <https://github.com/deliberium/mnemara>
