# Benchmark and Evaluation Methodology

Mnemara treats retrieval quality, portability safety, and control-plane latency as release artifacts rather than ad hoc spot checks.

## Ranked retrieval corpus

The checked-in corpus lives at `data/evaluation/ranking-corpus-v1.json`.

It explicitly covers the scenario slices called out in the roadmap:

- exact lookup
- duplicate-heavy recall
- recent-thread preference
- durable high-trust memory
- archival / cold-tier retrieval
- noisy distractors
- portability regression cases
- fairness/runtime guidance
- deployment transport guidance
- chronology reconstruction
- recurrence-pattern retrieval
- duration-and-boundary retrieval
- unresolved continuity follow-up
- contradiction and supersession recall
- preference change resolution
- operational drift tracking
- long-horizon task continuity

Each judged case has fixed relevance sets and deterministic expectations. The release gate is exercised by:

```bash
cargo test -p mnemara-core --test evaluation_corpus
```

The gate uses standard IR metrics at `k = 3`:

| Metric   | Gate      |
| -------- | --------- |
| Hit@3    | `>= 1.00` |
| Recall@3 | `>= 0.88` |
| MRR      | `>= 0.88` |
| NDCG@3   | `>= 0.90` |

## Episodic, planner, and lifecycle validation slices

The original ranked corpus remains the baseline release gate. Roadmap-era
retrieval features now add validation slices that are checked in alongside the
standard corpus instead of being treated as marketing-only behavior.

Current required slices are:

- chronology reconstruction and continuity follow-up prompts
- unresolved-thread recall using episodic continuity state
- contradiction, supersession, and historical-only policy recall
- preference change and operational drift recovery
- long-horizon task continuity with chronological reconstruction
- planner-stage and candidate-source explanation fidelity
- historical versus current visibility after compaction and retention
- lineage-preserving supersession and summary rollup behavior

The current repository evidence for those slices is exercised by:

```bash
cargo test --manifest-path /Users/kabudu/projex/deliberium-group/mnemara/Cargo.toml --workspace -- --test-threads=1
```

Important focused suites inside that workspace run include:

- `cargo test -p mnemara-core --lib` for planner and episodic score composition
- `cargo test -p mnemara-core --test evaluation_corpus` for the expanded checked-in ranked corpus
- `cargo test -p mnemara-server --test service_roundtrip` for transport-safe explanation and planner trace behavior
- `cargo test -p mnemara-server --test rollout_examples` for golden explanation payloads and documented query examples
- `cargo test -p mnemara-store-file --test replay_fixtures`
- `cargo test -p mnemara-store-sled --test replay_fixtures`

Those replay suites now act as the acceptance gate for:

- current-only versus historical-inclusive recall semantics
- duplicate consolidation with superseded visibility
- lifecycle-aware archival behavior under retention and compaction

## Reporting rules for new benchmark revisions

Future published benchmark artifact revisions must include separate reporting
for these roadmap-era slices rather than collapsing them into one aggregate
score.

Each published report should break out at least:

- baseline ranked retrieval quality
- chronology and continuity-sensitive retrieval quality
- recurrence and duration-sensitive retrieval quality
- salience-enabled versus salience-neutralized quality and recall latency on the same corpus
- explanation fidelity and planner-trace parity
- historical-versus-current lifecycle behavior
- recall latency by planner profile
- planner-stage latency for candidate generation, bounded graph expansion, and total planning
- provenance-policy comparisons with embedding mode held constant
- consolidation throughput on the fixed corpus
- recall latency while maintenance work is running
- archival and recovery timings for compaction and repair/import flows

If a capability is shipped before a new quantitative artifact revision exists,
the repository must still include the exact test command or checked-in evidence
used to validate that capability.

The release-candidate gate and rollout evidence rules for those checks are
documented in `docs/release-validation.md`.

## Published benchmark runner

The versioned benchmark runner lives at `crates/mnemara-store-sled/examples/publish_benchmarks.rs`.

Run it from the repository root with:

```bash
cargo run -p mnemara-store-sled --example publish_benchmarks -- \
  --output docs/benchmark-artifacts/benchmark-report-v1.json \
  --summary docs/benchmark-artifacts/benchmark-report-v1.md
```

The runner publishes:

- scorer family comparisons: `Profile` and `Curated`
- profile comparisons: `Balanced`, `LexicalFirst`, `ImportanceFirst`
- salience-isolated comparisons with the same scorer and planner profile while episodic salience is enabled versus neutralized
- planner-profile comparisons: `FastPath` and `ContinuityAware`
- planner-stage timings for candidate generation, graph expansion, and total planning
- fixed policy-profile comparisons with semantic mode held constant
- lifecycle maintenance timings for consolidation throughput, recall during maintenance, integrity checks, repair rebuilds, and recovery imports
- backend comparisons: `sled` and `file`
- stratified quality output per scenario slice
- operational timings for ingest, recall, snapshot, stats, export, dry-run compaction, and replace import
- environment disclosure: OS, architecture, and logical CPU count

The current published artifact revision includes those planner-profile,
episodic, and lifecycle-sensitive scenario tables. Future revisions should keep
regenerating the checked-in report files before any new headline performance
claims are made for historical or continuity-aware recall.

The current measurement profile is intentionally simple and reproducible:

| Dimension            | Value                                                       |
| -------------------- | ----------------------------------------------------------- |
| Upsert runs          | `6`                                                         |
| Recall loops         | `6`                                                         |
| Admin-operation runs | `4`                                                         |
| Embedding mode       | `DeterministicLocal`                                        |
| Corpus export scope  | `tenant=default`, `namespace=evaluation`, archived included |

Latency summaries are published as mean, p50, p95, and max. Ingest throughput is recorded as records per second.
Lifecycle maintenance summaries now also publish consolidation throughput as
records per second on the fixed corpus and separate timing for integrity,
repair, and recovery flows.

## Portability and admin validation

Portable package safety is covered by:

```bash
cargo test -p mnemara-store-sled portable_ -- --nocapture
```

That suite now exercises:

- cross-backend roundtrip import/export
- validate-only mode
- dry-run safety
- unsupported package-version handling
- structured import failure reporting

The same portable package format is also the recommended backend-seeding mechanism for repeatable benchmark runs and migration rehearsals: export once, then import with `Validate`, `Merge`, `Replace`, or `dry_run` into any supported backend under test.

Server-side observability and runtime controls are covered by:

```bash
cargo test -p mnemara-server admin_trace_endpoints_expose_recent_operations -- --nocapture
```

The HTTP admin surface now exposes filtered traces, runtime admission state, retention saturation, and trace/backend correlation metadata.
