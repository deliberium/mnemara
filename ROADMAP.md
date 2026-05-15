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

- [ ] optional advanced scoring or embedding plug-ins
- [ ] broader typed graph expansion and multi-hop retrieval improvements beyond
      the now-shipped bounded continuity-aware hop expansion

### Platform and Runtime Expansion

- [ ] additional non-Rust SDKs and registry publication automation
- [ ] C ABI / FFI surface
- [ ] background maintenance orchestration beyond the now-shipped bounded
      repair, integrity, compaction, and recovery tooling
- [ ] remote replication, snapshot shipping, and controlled multi-node
      deployments without losing local-first guarantees

### Operator Experience

- [ ] admin tooling for inspecting episode graphs, memory lineage, and
      consolidation decisions

## Scope Rule

If a feature is not required for the first release to function as a local-first embedded library and gRPC daemon, it belongs here until it is promoted into an implementation plan.
