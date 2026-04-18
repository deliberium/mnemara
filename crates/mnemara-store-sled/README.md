# mnemara-store-sled

`mnemara-store-sled` provides the embedded sled-backed implementation of the `mnemara-core::MemoryStore` trait.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-store-sled
```

## When to use it

Use the sled backend when you want an embedded store with stronger indexing and operational characteristics than the file-backed compatibility store.

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
- supports compaction, retention, integrity checks, repair flows, and portable import/export

Project documentation: <https://github.com/deliberium/mnemara>
