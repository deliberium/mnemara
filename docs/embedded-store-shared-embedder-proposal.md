# Embedded Store Shared Embedder Injection

Status: implemented on the current main branch.

## Summary

`mnemara` now exposes a shared semantic embedder injection seam for the embedded file and sled stores.

Embedded callers can keep `mnemara`'s planner, scorer, trace, and explanation model while supplying an application-owned `Arc<dyn SemanticEmbedder>` at store construction time. When the shared embedder is not configured, both stores continue to fall back to `EngineConfig` embedding settings.

## Shipped API

Both embedded store configs now support:

```rust
use std::sync::Arc;
use mnemara::{FileStoreConfig, SemanticEmbedder, SledStoreConfig};

let embedder: Arc<dyn SemanticEmbedder> = todo!();

let file_config = FileStoreConfig::new("./data")
    .with_shared_embedder(Arc::clone(&embedder), "embedding_provider=my_provider");

let sled_config = SledStoreConfig::new("./data")
    .with_shared_embedder(embedder, "embedding_provider=my_provider");
```

Internally, both stores now keep optional shared embedder config and construct recall planning through a store-local helper that chooses between:

- `RecallPlanner::with_shared_embedder(...)` when a shared embedder is configured
- `RecallPlanner::from_engine_config(...)` otherwise

## Behavior Guarantees

- backward compatible for existing embedded callers
- no behavior change when `with_shared_embedder(...)` is unused
- no daemon or protocol changes required
- engine-config tuning remains authoritative for planner profile, policy profile, scorer kind, graph hops, and other non-embedder settings
- provider notes remain visible through recall explanations when the shared embedder is active

## Validation

The implementation is covered by targeted embedded-store tests in both backends:

- `mnemara-store-sled`: `shared_embedder_injection_can_enable_semantic_recall_without_engine_embedding`
- `mnemara-store-file`: `shared_embedder_injection_can_enable_semantic_recall_without_engine_embedding`

Those tests verify that semantic recall works with engine-level embeddings disabled when a custom shared embedder is injected.

## Deliberium Integration Note

This is the upstream seam Deliberium needed in order to reuse its local MiniLM text embedder for Cortex recall without forking embedded store internals or switching to daemon mode.
