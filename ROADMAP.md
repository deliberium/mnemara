# Mnemara Roadmap

This document tracks capabilities that are intentionally outside the first release scope.

## Direction

The long-term goal is not vague "human-like recall." The goal is durable,
inspectable episodic memory for agents: what happened, when it happened, why it
mattered, how it connects to prior events, and what should influence future
behavior.

## Long-Term

### Episodic Retrieval and Memory Intelligence

- [x] Deepen temporal reasoning beyond the now-shipped episode chronology,
      recurrence, duration, and boundary cues so recall can support richer
      before/after constraints, longer-horizon sequencing, and stronger task or
      session boundary reasoning. Shipped as relative before/after record
      anchors plus boundary-label and recurrence-key recall filters.
- [x] Extend contradiction and drift handling beyond the now-shipped current,
      historical, and superseded distinction into broader conflict-review and
      operator-resolution workflows. Shipped as additive conflict-review
      metadata, unresolved-conflict recall filters, resolution-kind audit
      filters, and explanation channel disclosure.

### Retrieval and Ranking Expansion

- [x] optional advanced scoring or embedding plug-ins. Shipped as configurable
      scorer families/profiles plus a shared semantic embedder seam and
      deterministic local reference embedder.
- [x] broader typed graph expansion and multi-hop retrieval improvements beyond
      the now-shipped bounded continuity-aware hop expansion
      (`graph_expansion_max_hops`). Shipped as trace-visible graph relation
      reasons for same-episode, chronology, causal, related, and lineage
      expansion candidates.

### Platform and Runtime Expansion

- [x] additional non-Rust SDKs and registry publication automation. Shipped as
      JavaScript and dependency-free Python HTTP SDKs plus release validation
      and `sdk-publish` automation for npm and PyPI registry publication.
- [x] C ABI / FFI surface. Shipped as the `mnemara-ffi` crate with a JSON-based
      C ABI over the sled-backed store.
- [x] background maintenance orchestration beyond the now-shipped bounded
      repair, integrity, compaction, and recovery tooling. Shipped as the
      `RunMaintenance` admin operation plus daemon background scheduling env
      controls.
- [x] remote replication, snapshot shipping, and controlled multi-node
      deployments without losing local-first guarantees. Shipped as portable
      snapshot shipping to remote daemon import endpoints, defaulting to
      validation/dry-run friendly semantics.

### Operator Experience

- [x] admin tooling for inspecting episode graphs, memory lineage, and
      consolidation decisions. Shipped first as read-only `/admin/graph`
      inspection for episode, chronology, causal, related, lineage, and conflict
      edges, with SDK access and trace correlation.

## Current Implementation Plan

The next promoted work focuses on high-value features that strengthen Mnemara's
inspectable-memory position without making the core product application-specific.

- [x] first-class memory evaluation harness for judged recall cases, expected
      and disallowed records, explanation assertions, and repeatable quality
      reports across embedded and daemon-backed stores. Shipped as core
      `RecallEvaluationCase` helpers and an async `run_recall_evaluation`
      runner over any `MemoryStore`.
- [x] append-only memory changefeed/watch API for upserts, lifecycle changes,
      deletes, compaction, import, and maintenance-derived mutations so
      supervisors, audit tools, sync adapters, and operator UIs can consume
      memory changes without owning store internals. Shipped first as
      backend-level append-only events for direct mutations, an additive
      `MemoryStore::changefeed` API, `/admin/changefeed`, and JavaScript/Python
      SDK helpers.
- [x] time-travel recall that can answer recall queries as of a timestamp using
      versioned memory history while preserving current recall defaults. Shipped
      as additive `TimeTravelRecallRequest`, `MemoryStore::recall_as_of`,
      `/memory/recall-as-of`, gRPC `RecallAsOf`, and JavaScript/Python SDK
      helpers.
- [x] inspectable memory summarization workbench that proposes consolidation
      rollups with source records, preserved facts, confidence, provenance, and
      review state. Shipped first as deterministic synthesis proposals that
      create draft `Summary` records with source lineage, dry-run defaults,
      HTTP `/admin/synthesize`, gRPC `Synthesize`, maintenance opt-in, and
      JavaScript/Python SDK helpers.

## Future Candidates

- [ ] policy-aware recall modes that bundle lifecycle, trust, provenance,
      historical visibility, conflict status, and ranking posture into named
      intent-specific safety profiles
- [ ] portable recall-debugger artifacts that capture query text, effective
      engine config, candidate stages, filter exclusions, score components,
      graph expansion paths, backend/version metadata, and final ranking for
      replayable investigation
- [ ] read-only memory hygiene advisor for duplicate clusters, stale but
      frequently recalled memories, unresolved conflicts, high-impact
      unverified records, orphaned episode fragments, namespace growth, and weak
      provenance signals
- [ ] encrypted and signed portable snapshot packages with selective export,
      tamper detection, signature verification, and dry-run import validation
- [ ] schema migration and compatibility reports covering store schema version,
      record contract version distribution, deprecated feature usage, migration
      dry-runs, and target-version compatibility
- [ ] capability-limited daemon tokens with tenant, namespace, read/write/admin,
      lifecycle-operation, expiration, and audit-identity constraints

## Scope Rule

If a feature is not required for the first release to function as a local-first embedded library and gRPC daemon, it belongs here until it is promoted into an implementation plan.
