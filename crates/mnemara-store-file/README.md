# mnemara-store-file

`mnemara-store-file` provides a compatibility-oriented file-backed implementation of the `mnemara-core::MemoryStore` trait.

Choose this crate when you want backend-neutral persistence with ordinary files and readable JSON artifacts, especially for local development, fixtures, and portability workflows.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-store-file
```

## When to use it

Use the file store when you want a simple local persistence model backed by ordinary files and directories rather than an embedded database.

It preserves the same typed memory surface as the rest of the workspace, including episodic context, lineage, historical state, lifecycle controls, snapshots, and portable import or export packages.

## Minimal example

```rust
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};

let store = FileMemoryStore::open(FileStoreConfig::new("./data/mnemara-file"))?;
# let _ = store;
# Ok::<(), mnemara_core::Error>(())
```

## Notes

- depends on `mnemara-core` for the domain model and store traits
- suitable for local-first development, tests, and simple embedded deployments
- persists episodic records, additive lifecycle state, and lineage metadata in a backend-neutral JSON representation
- supports archive, suppress, recover, snapshot, stats, integrity, repair, compaction, retention, and delete flows through the shared store trait
- portable import and export flows round-trip with the sled backend

Workspace and deployment docs: <https://github.com/deliberium/mnemara>
