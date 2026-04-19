# mnemara-store-sled

`mnemara-store-sled` provides the embedded sled-backed implementation of the `mnemara-core::MemoryStore` trait.

Choose this crate when you want the primary embedded backend for production-style local deployments, richer indexing, and the same episodic or lifecycle semantics used by the daemon.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-store-sled
```

## When to use it

Use the sled backend when you want an embedded store with stronger indexing and operational characteristics than the file-backed compatibility store.

It is the primary local-first backend for continuity-aware recall, lifecycle controls, and daemon-backed deployments.

## Minimal example

```rust
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};

let store = SledMemoryStore::open(SledStoreConfig::new("./data/mnemara-sled"))?;
# let _ = store;
# Ok::<(), mnemara_core::Error>(())
```

## Notes

- depends on `mnemara-core` for memory types, queries, and traits
- used by the standalone `mnemara-server` daemon
- indexes episodic context, historical state, lineage, and replay-safe idempotent writes for richer recall behavior
- supports archive, suppress, recover, compaction, retention, stats, integrity checks, repair flows, and portable import or export
- exposes the same backend-neutral store contract as the file backend while favoring stronger embedded runtime characteristics

Project documentation: <https://github.com/deliberium/mnemara>
