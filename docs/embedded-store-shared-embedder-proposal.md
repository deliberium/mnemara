# Embedded Store Shared Embedder Injection Proposal

## Proposed Issue Title

Expose a shared semantic embedder injection seam for embedded file and sled stores.

## Summary

`mnemara-core` already exposes a public semantic embedding seam through `SemanticEmbedder`, `SharedSemanticEmbedder`, `ConfiguredSemanticEmbedder::shared`, `ConfiguredRecallScorer::with_shared_embedder`, and `RecallPlanner::with_shared_embedder`.

Embedded callers cannot currently use that seam through `mnemara-store-file` or `mnemara-store-sled`. Both stores accept only `EngineConfig` and construct recall planning internally from `RecallPlanner::from_engine_config(...)`, which limits embedded applications to the built-in embedding providers declared by engine configuration.

That is a useful default, but it blocks a practical integration class: applications that already ship a local text embedding backend and want `mnemara` recall to reuse it without switching to daemon mode or forking store internals.

## Why This Matters

- preserves local-first embedded deployment
- avoids duplicate embedding stacks inside host applications
- keeps retrieval explainability intact because the planner and scorer remain `mnemara`'s
- fits the roadmap item for optional advanced scoring or embedding plug-ins
- improves compatibility for products that already own model lifecycle, caching, or asset rollout

## Current Limitation

Today the embedded-store path looks roughly like this:

- the caller builds `FileStoreConfig` or `SledStoreConfig`
- the caller may supply `EngineConfig`
- the store later builds `RecallPlanner::from_engine_config(...)`
- the planner builds a scorer from engine config
- the scorer builds a configured embedder from engine config

That means there is no store-level way to inject `Arc<dyn SemanticEmbedder>` for embedded recall.

## Proposal

Add an optional shared embedder field to the embedded store configuration types and use it when constructing the recall planner.

### Public API Shape

Minimal additive API:

```rust
use std::sync::Arc;
use mnemara_core::SemanticEmbedder;

impl FileStoreConfig {
    pub fn with_shared_embedder(
        mut self,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        // store embedder for later planner/scorer construction
        self
    }
}

impl SledStoreConfig {
    pub fn with_shared_embedder(
        mut self,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        // store embedder for later planner/scorer construction
        self
    }
}
```

Internally, the stores would construct recall planning like this:

```rust
let planner = if let Some(shared) = self.shared_embedder.as_ref() {
    RecallPlanner::with_shared_embedder(
        &self.engine_config,
        shared.clone(),
        self.shared_embedder_provider_note.clone(),
    )
} else {
    RecallPlanner::from_engine_config(&self.engine_config)
};
```

The exact storage type is flexible. The important part is preserving an application-owned embedder for planner and scorer construction without changing request-time recall APIs.

## Compatibility Requirements

- fully backward compatible for existing embedded callers
- no behavior change when `with_shared_embedder(...)` is unused
- no daemon or protocol changes required
- keep engine-config tuning authoritative for planner profile, policy profile, scorer kind, graph hops, and other non-embedder settings
- surface the provider note in traces or runtime tuning info when a shared embedder is active

## Non-Goals

- adding per-query embedder overrides
- changing daemon configuration or transport APIs in the first pass
- introducing opaque remote embedding calls into the embedded path
- bypassing the existing planner, scorer, trace, or explanation model

## Acceptance Criteria

- `FileStoreConfig` supports configuring a shared semantic embedder
- `SledStoreConfig` supports configuring a shared semantic embedder
- embedded recall uses the shared embedder when present
- recall behavior falls back to `EngineConfig` embedding settings when absent
- traces or tuning output clearly indicate that a shared embedder is active
- tests cover parity across file and sled stores

## Validation

- unit tests proving planner construction uses shared embedder injection when configured
- regression tests proving existing engine-config-only behavior is unchanged
- embedded-store tests for at least one custom deterministic test embedder
- documentation update in the user guide showing embedded usage

## Suggested Patch Breakdown

1. Add optional shared-embedder configuration fields and builders to `FileStoreConfig` and `SledStoreConfig`.
2. Add a small helper in each store for planner construction so the injection logic is centralized.
3. Update recall code paths to use that helper instead of calling `RecallPlanner::from_engine_config(...)` directly.
4. Add tests for file and sled backends using a fixed deterministic custom embedder.
5. Document the embedded usage pattern and provider-note visibility.

## Alternatives Considered

Keep relying on `EngineConfig` only:
This preserves simplicity, but it makes the public shared-embedder seam unavailable to embedded users, which is inconsistent with the advertised extension surface.

Require daemon mode for custom embedders:
This avoids changing embedded stores, but it weakens the local embedded story for applications that already manage their own local models.

Expose a lower-level recall method that accepts a planner per call:
This is more flexible, but it is a materially larger API surface and operational burden than a store-construction-time injection seam.

## Ready-To-File Issue Body

```md
## Summary

`mnemara-core` already exposes `SemanticEmbedder`, `ConfiguredSemanticEmbedder::shared`, `ConfiguredRecallScorer::with_shared_embedder`, and `RecallPlanner::with_shared_embedder`, but embedded callers cannot currently use that seam through `mnemara-store-file` or `mnemara-store-sled`.

Both embedded stores accept `EngineConfig` and later construct recall planning with `RecallPlanner::from_engine_config(...)`, so embedded applications are limited to the built-in embedding providers declared by engine config.

## Proposal

Add an optional shared embedder field plus `with_shared_embedder(...)` builder to `FileStoreConfig` and `SledStoreConfig`, and use that injected embedder when constructing the planner/scorer for embedded recall.

## Why

- keeps embedded deployments local-first
- lets host applications reuse an existing local text embedding backend
- avoids duplicate embedding stacks and asset rollout
- preserves `mnemara` planner, scorer, trace, and explanation behavior

## Acceptance Criteria

- additive, backward-compatible builder on both embedded store configs
- embedded recall uses shared embedder when configured
- existing `EngineConfig` behavior remains unchanged when not configured
- file and sled tests cover custom embedder injection
- docs show embedded usage
```
