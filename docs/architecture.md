# Mnemara Architecture

## Layers

The standalone product is intended to evolve into these layers:

1. core domain model and traits
2. embedded storage backends
3. retrieval and ranking planner
4. maintenance and compaction subsystem
5. service transport layer
6. service runtime and deployment shell
7. language SDKs and adapters

## Product Boundary

Mnemara should be memory-centric rather than application-centric.

That means the core product should not assume:

- application-specific orchestration flows
- voice-specific heuristics
- application-specific auth contracts
- product-specific UI concepts

Those should be handled in adapters or companion services.

## Planned Runtime Modes

### Embedded

- direct crate dependency
- in-process APIs
- lowest-latency path

### Daemon

- gRPC over Unix domain sockets or TCP
- optional HTTP admin surface
- cross-language adoption path

The first server implementation now exists as `crates/mnemara-server`, which exposes the protobuf-defined `MemoryService` over tonic with upsert, batch upsert, recall, compact, snapshot, and delete operations.

Future daemon capabilities beyond the first release are tracked in [ROADMAP.md](../ROADMAP.md).

## Design Constraints

- local-first operational posture
- explainable retrieval
- stable schema evolution
- inspectable compaction behavior
- policy-driven retention and lifecycle enforcement
- first-class observability

## Retrieval Model

The current retrieval stack is no longer a single lexical scorer.

Shipped retrieval behavior now combines:

- lexical and semantic matching
- metadata and temporal weighting
- episodic continuity and salience signals
- provenance-aware policy adjustments
- planner-driven candidate expansion with bounded graph-style continuity hops

The planner currently exposes two additive profiles:

1. `FastPath` for low-latency default retrieval
2. `ContinuityAware` for episode-sensitive and follow-up style queries

The scorer layer now also exposes workload policy profiles:

1. `General` as the default balanced policy
2. `Support` for stronger preference toward current, verified, and pinned facts
3. `Research` for broader tolerance of historical and derived context
4. `Assistant` for continuity-sensitive general assistant recall
5. `AutonomousAgent` for stricter provenance in execution-oriented workflows

The embedding seam is also additive rather than mandatory. Current shipped
providers are:

1. `Disabled` as the safe fallback with no semantic channel contribution
2. `DeterministicLocal` for reproducible local semantic scoring and tests

Downstream Rust callers can also inject a custom semantic provider through the
shared scorer and planner constructors instead of being limited to the built-in
engine-config enum. The supported extension path is:

1. implement `SemanticEmbedder`
2. wrap it in an `Arc`
3. inject it with `ProfileRecallScorer::with_shared_embedder`, `CuratedRecallScorer::with_shared_embedder`, `ConfiguredRecallScorer::with_shared_embedder`, or `RecallPlanner::with_shared_embedder`

That custom path remains additive: the built-in config-driven providers still
work unchanged, and explanation payloads continue to expose the active provider
through the configured provider note string.

Planner expansion remains bounded by `graph_expansion_max_hops` and now applies
typed relation families across episode membership, chronology, causal links,
related-record links, and lineage references.

Recall explanations and planning traces expose the effective planner profile,
effective policy profile, selected channels, candidate sources, planner stages,
and per-hit score breakdowns.

## Episodic Memory Model

`MemoryRecord` can now carry an optional `EpisodeContext` alongside the base
record payload.

That episodic context captures:

- an explicit schema version for the episode contract
- episode identifiers and continuity state
- goal and outcome summaries
- actor participation
- recurrence keys and interval hints for recurring episodes
- duration and boundary cues through explicit timestamps and boundary labels
- causal, previous, next, and related record references
- salience signals such as reuse, novelty, and unresolved weight
- optional affective annotations with explicit provenance

These fields are additive and remain optional so existing stored records and
clients stay compatible.

Episode membership stays record-first and additive. When an `episode` object is
present, current write validation requires a non-empty `episode_id`, the
current supported `schema_version`, chronological timeline bounds, inclusion of
the owning `actor_id` when `actor_ids` is provided, and non-self-referential
`previous`, `next`, `causal`, and `related` links.

Derived affective annotations are also validated at write time. They must keep
confidence within `0.0..=1.0`, remain below certainty when provenance is
`Derived`, and avoid empty tone or sentiment strings.

## Lifecycle and Consolidation

The maintenance layer now distinguishes memory quality state from historical
state.

Quality state still answers questions such as whether a record is active,
verified, archived, suppressed, or deleted. Historical state now answers a
different question: whether the record is the current view, preserved as
historical context, or superseded by a newer consolidated product.

Lineage links connect derived summaries and superseded records to the source
records that produced them. Compaction can therefore emit summary records,
archive or supersede duplicates, and preserve provenance instead of treating
maintenance as destructive cleanup.

Recall queries can now explicitly request:

- current-only views
- historical-only views
- mixed current and historical recall
- lineage-focused retrieval around a specific record

## Service and Operator Surfaces

The daemon and protocol layers surface the same retrieval and lifecycle model
used by the embedded stores.

Operators can now inspect:

- lifecycle-aware maintenance counters
- compaction reports that include superseded-record and lineage-link counts
- recall traces with planner-stage and candidate-source detail
- transport-safe lifecycle and episodic fields over protobuf and HTTP/JSON

This keeps embedded mode, daemon mode, and backend implementations aligned on a
single explainable retrieval and lifecycle contract.
