# Benchmark report v1

Environment: `macos` `aarch64` with 10 logical CPUs.

## Profile / Balanced

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 80.33 | 1.23 | 9.59 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 12.05 | 0.66 | 3.76 |

## Profile / LexicalFirst

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 0.94 | 0.96 | 78.93 | 1.02 | 12.47 |
| file | 1.00 | 1.00 | 0.94 | 0.96 | 19.31 | 0.69 | 4.34 |

## Curated / Balanced

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 88.83 | 1.06 | 12.34 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 16.70 | 0.66 | 3.70 |

## Curated / ImportanceFirst

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 0.99 | 80.22 | 1.19 | 11.12 |
| file | 1.00 | 1.00 | 1.00 | 0.99 | 19.97 | 0.72 | 3.08 |

