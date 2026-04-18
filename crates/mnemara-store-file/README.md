# mnemara-store-file

`mnemara-store-file` provides a compatibility-oriented file-backed implementation of the `mnemara-core::MemoryStore` trait.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-store-file
```

## When to use it

Use the file store when you want a simple local persistence model backed by ordinary files and directories rather than an embedded database.

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
- portable import and export flows round-trip with the sled backend

Workspace and deployment docs: <https://github.com/deliberium/mnemara>
