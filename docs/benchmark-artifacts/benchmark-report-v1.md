# Benchmark report v1

Environment: `macos` `aarch64` with 10 logical CPUs.

## Profile / Balanced / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 175.43 | 1.41 | 11.32 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 100.89 | 1.51 | 6.29 |

## Profile / Balanced / ContinuityAware / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 174.76 | 1.38 | 13.58 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 44.89 | 1.51 | 10.87 |

## Profile / LexicalFirst / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 0.97 | 0.98 | 184.16 | 2.75 | 11.85 |
| file | 1.00 | 1.00 | 0.97 | 0.98 | 49.50 | 1.53 | 14.67 |

## Curated / Balanced / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 185.46 | 1.40 | 14.28 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 42.07 | 1.55 | 6.61 |

## Curated / ImportanceFirst / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 184.97 | 1.30 | 15.05 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 50.46 | 1.53 | 5.20 |

## Salience-isolated comparison

| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Profile / Balanced | ContinuityAware | General | salience_enabled | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.43 |
| Profile / Balanced | ContinuityAware | General | salience_enabled | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.53 |
| Profile / Balanced | ContinuityAware | General | salience_neutralized | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.38 |
| Profile / Balanced | ContinuityAware | General | salience_neutralized | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.50 |

## Planner stage timings

| scorer / profile | planner | policy | candidate mean ms | graph p95 ms | total mean ms | mean seeded | mean expanded | max hops |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Profile / Balanced | FastPath | General | 0.5632 | 0.0125 | 0.5735 | 28.78 | 0.00 | 0 |
| Profile / Balanced | ContinuityAware | General | 0.5647 | 0.0299 | 0.5868 | 28.78 | 0.11 | 1 |
| Profile / Balanced | ContinuityAware | Support | 0.5668 | 0.0303 | 0.5893 | 28.78 | 0.11 | 1 |

## Provenance policy profile comparison

| policy profile | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| General | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.44 |
| General | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.48 |
| Support | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.34 |
| Support | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.53 |
| Research | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.40 |
| Research | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.55 |
| Assistant | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.31 |
| Assistant | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.54 |
| AutonomousAgent | sled | 1.00 | 1.00 | 1.00 | 1.00 | 1.38 |
| AutonomousAgent | file | 1.00 | 1.00 | 1.00 | 1.00 | 1.55 |

## Lifecycle maintenance timings

| backend | records | consolidation rec/s | consolidation mean ms | recall-during-maintenance p95 ms | integrity mean ms | repair mean ms | recovery import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 29 | 3309.10 | 8.76 | 2.01 | 3.28 | 8.34 | 13.82 |
| file | 29 | 4707.94 | 6.16 | 1.68 | 2.22 | 4.44 | 5.45 |
