# Benchmark report v1

Environment: `macos` `aarch64` with 10 logical CPUs.

## Salience-isolated comparison

| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |

## Shared embedder injection comparison

| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |

## Planner stage timings

| scorer / profile | planner | policy | candidate mean ms | graph p95 ms | total mean ms | mean seeded | mean expanded | max hops |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |

## Provenance policy profile comparison

| policy profile | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |

## Lifecycle maintenance timings

| backend | records | consolidation rec/s | consolidation mean ms | recall-during-maintenance p95 ms | integrity mean ms | repair mean ms | recovery import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |

## Synthesis proposal timings

| backend | records | groups | dry-run rec/s | dry-run mean ms | filtered dry-run mean ms | apply rec/s | apply mean ms | proposals | lineage links |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 100 | 5 | 470680.71 | 0.21 | 0.16 | 20568.90 | 4.86 | 5.00 | 60.00 |
| file | 100 | 5 | 79548.55 | 1.26 | 1.22 | 42562.62 | 2.35 | 5.00 | 60.00 |
| sled | 500 | 25 | 486213.65 | 1.03 | 0.80 | 47537.18 | 10.52 | 25.00 | 300.00 |
| file | 500 | 25 | 78879.78 | 6.34 | 6.14 | 30911.26 | 16.18 | 25.00 | 300.00 |
| sled | 1000 | 50 | 280760.86 | 3.56 | 2.77 | 62648.22 | 15.96 | 50.00 | 600.00 |
| file | 1000 | 50 | 76928.38 | 13.00 | 12.79 | 23291.03 | 42.93 | 50.00 | 600.00 |
