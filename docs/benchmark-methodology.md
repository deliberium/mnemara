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

Each judged case has fixed relevance sets and deterministic expectations. The release gate is exercised by:

```bash
cargo test -p mnemara-core --test evaluation_corpus
```

The gate uses standard IR metrics at `k = 3`:

| Metric | Gate |
| --- | --- |
| Hit@3 | `>= 1.00` |
| Recall@3 | `>= 0.88` |
| MRR | `>= 0.88` |
| NDCG@3 | `>= 0.90` |

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
- backend comparisons: `sled` and `file`
- stratified quality output per scenario slice
- operational timings for ingest, recall, snapshot, stats, export, dry-run compaction, and replace import
- environment disclosure: OS, architecture, and logical CPU count

The current measurement profile is intentionally simple and reproducible:

| Dimension | Value |
| --- | --- |
| Upsert runs | `6` |
| Recall loops | `6` |
| Admin-operation runs | `4` |
| Embedding mode | `DeterministicLocal` |
| Corpus export scope | `tenant=default`, `namespace=evaluation`, archived included |

Latency summaries are published as mean, p50, p95, and max. Ingest throughput is recorded as records per second.

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
