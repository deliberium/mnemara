# Benchmark and Evaluation Results

The current published artifacts are:

- `docs/benchmark-artifacts/benchmark-report-v1.json`
- `docs/benchmark-artifacts/benchmark-report-v1.md`

They were generated with:

```bash
cargo run -p mnemara-store-sled --example publish_benchmarks -- \
  --output docs/benchmark-artifacts/benchmark-report-v1.json \
  --summary docs/benchmark-artifacts/benchmark-report-v1.md
```

## Environment

| Field | Value |
| --- | --- |
| OS | `macos` |
| Architecture | `aarch64` |
| Logical CPUs | `10` |

## Quality summary

Across the full ranked corpus, the published comparison run produced:

| Scorer / profile | Backend | Hit@3 | Recall@3 | MRR | NDCG@3 |
| --- | --- | ---: | ---: | ---: | ---: |
| Profile / Balanced | sled | `1.00` | `1.00` | `1.00` | `1.00` |
| Profile / Balanced | file | `1.00` | `1.00` | `1.00` | `1.00` |
| Profile / LexicalFirst | sled | `1.00` | `1.00` | `0.94` | `0.96` |
| Profile / LexicalFirst | file | `1.00` | `1.00` | `0.94` | `0.96` |
| Curated / Balanced | sled | `1.00` | `1.00` | `1.00` | `1.00` |
| Curated / Balanced | file | `1.00` | `1.00` | `1.00` | `1.00` |
| Curated / ImportanceFirst | sled | `1.00` | `1.00` | `1.00` | `0.99` |
| Curated / ImportanceFirst | file | `1.00` | `1.00` | `1.00` | `0.99` |

Every published run also includes stratified scenario results in the JSON artifact for:

- exact lookup
- duplicate-heavy
- recent-thread
- durable high-trust
- archival cold-tier
- noisy distractor
- portability regression
- fairness runtime
- deployment transport

## Performance summary

Headline latency figures from `benchmark-report-v1.md`:

| Scorer / profile | Backend | Ingest mean ms | Recall p95 ms | Import mean ms |
| --- | --- | ---: | ---: | ---: |
| Profile / Balanced | sled | `80.33` | `1.23` | `9.59` |
| Profile / Balanced | file | `12.05` | `0.66` | `3.76` |
| Profile / LexicalFirst | sled | `78.93` | `1.02` | `12.47` |
| Profile / LexicalFirst | file | `19.31` | `0.69` | `4.34` |
| Curated / Balanced | sled | `88.83` | `1.06` | `12.34` |
| Curated / Balanced | file | `16.70` | `0.66` | `3.70` |
| Curated / ImportanceFirst | sled | `80.22` | `1.19` | `11.12` |
| Curated / ImportanceFirst | file | `19.97` | `0.72` | `3.08` |

The JSON artifact also contains:

- ingest throughput per second
- recall mean, p50, p95, and max
- snapshot, stats, export, dry-run compaction, and replace-import timings
- exported storage-byte totals

## Portability and admin status

The release evidence now includes:

- validate-only import reports with no writes applied
- dry-run import reports with structured failures
- package-version compatibility reporting
- file-to-sled roundtrip coverage
- admin trace filtering plus runtime fairness/retention status
