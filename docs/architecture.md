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
