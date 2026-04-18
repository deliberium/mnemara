# Mnemara Roadmap

This document tracks capabilities that are intentionally outside the first release scope.

## Direction

The long-term goal is not vague "human-like recall." The goal is durable,
inspectable episodic memory for agents: what happened, when it happened, why it
mattered, how it connects to prior events, and what should influence future
behavior.

## Long-Term

### Episodic Retrieval and Memory Intelligence

- [ ] Introduce first-class episode structures that group related events,
      actors, artifacts, goals, and outcomes rather than treating every memory
      as an isolated record.
- [ ] Add temporal reasoning beyond freshness scoring: sequence, duration,
      recurrence, before/after constraints, and session or task boundaries.
- [ ] Build a salience model that combines authored importance,
      reinforcement-from-reuse, novelty, unresolved state, and goal relevance.
- [ ] Add narrative continuity primitives so recall can surface threads such as
      "what led to this", "what changed", "what is still unresolved", and
      "what happened next".
- [ ] Support optional affective and interpersonal signals as explicit,
      inspectable metadata or derived annotations, not hidden magic. This should
      capture tone, urgency, confidence, tension, and user sentiment when the
      source data supports it.
- [ ] Expand retrieval planning to blend lexical, semantic, metadata, graph,
      temporal, and episodic signals with clear score breakdowns and policy
      traces.
- [ ] Add memory consolidation flows that promote raw events into summaries,
      stable facts, preferences, open loops, and task-relevant working sets.
- [ ] Add contradiction, drift, and version-awareness so recall can distinguish
      between current truth, superseded belief, and historical context.
- [ ] Add benchmark suites for follow-up continuity, preference drift,
      contradiction handling, long-horizon task execution, and narrative recall
      quality.

### Retrieval and Ranking Expansion

- [ ] optional advanced scoring or embedding plug-ins
- [ ] typed graph expansion and multi-hop retrieval improvements
- [ ] retrieval policy profiles tuned for support, research, assistant, and
      autonomous-agent workloads
- [ ] provenance-aware ranking that can prefer verified, pinned, or recent
      sources depending on policy

### Platform and Runtime Expansion

- [ ] additional non-Rust SDKs and registry publication automation
- [ ] C ABI / FFI surface
- [ ] background maintenance orchestration and repair tooling
- [ ] remote replication, snapshot shipping, and controlled multi-node
      deployments without losing local-first guarantees

### Operator Experience

- [ ] richer observability for recall explanations, maintenance decisions, and
      policy outcomes
- [ ] admin tooling for inspecting episode graphs, memory lineage, and
      consolidation decisions
- [ ] safer lifecycle controls for retention, suppression, archival, and
      tenant-scoped recovery

## Scope Rule

If a feature is not required for the first release to function as a local-first embedded library and gRPC daemon, it belongs here until it is promoted into an implementation plan.
