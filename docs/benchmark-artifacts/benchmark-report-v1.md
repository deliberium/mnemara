# Benchmark report v1

Environment: `linux` `x86_64` with 4 logical CPUs.

## Profile / Balanced / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.90 | 2.77 | 3.27 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 36.62 | 2.97 | 3.05 |

## Profile / Balanced / ContinuityAware / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.91 | 2.80 | 3.17 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 36.55 | 2.99 | 3.02 |

## Profile / LexicalFirst / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 0.97 | 0.98 | 39.36 | 2.76 | 3.10 |
| file | 1.00 | 1.00 | 0.97 | 0.98 | 36.59 | 2.98 | 3.08 |

## Curated / Balanced / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.47 | 2.85 | 3.18 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 36.57 | 3.06 | 3.03 |

## Curated / ImportanceFirst / FastPath / General

| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.48 | 2.84 | 3.17 |
| file | 1.00 | 1.00 | 1.00 | 1.00 | 36.59 | 3.05 | 3.03 |

## Salience-isolated comparison

| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Profile / Balanced | ContinuityAware | General | salience_enabled | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.78 |
| Profile / Balanced | ContinuityAware | General | salience_enabled | file | 1.00 | 1.00 | 1.00 | 1.00 | 3.02 |
| Profile / Balanced | ContinuityAware | General | salience_neutralized | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.79 |
| Profile / Balanced | ContinuityAware | General | salience_neutralized | file | 1.00 | 1.00 | 1.00 | 1.00 | 3.00 |

## Shared embedder injection comparison

| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Profile / Balanced | ContinuityAware | General | engine_config_deterministic_local | sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.72 | 2.80 |
| Profile / Balanced | ContinuityAware | General | engine_config_deterministic_local | file | 1.00 | 1.00 | 1.00 | 1.00 | 36.62 | 3.01 |
| Profile / Balanced | ContinuityAware | General | shared_injected_deterministic_local | sled | 1.00 | 1.00 | 1.00 | 1.00 | 39.41 | 2.77 |
| Profile / Balanced | ContinuityAware | General | shared_injected_deterministic_local | file | 1.00 | 1.00 | 1.00 | 1.00 | 36.45 | 3.00 |

## Planner stage timings

| scorer / profile | planner | policy | candidate mean ms | graph p95 ms | total mean ms | mean seeded | mean expanded | max hops |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Profile / Balanced | FastPath | General | 1.5257 | 0.0255 | 1.5483 | 28.78 | 0.00 | 0 |
| Profile / Balanced | ContinuityAware | General | 1.5237 | 0.0670 | 1.5687 | 28.78 | 0.11 | 1 |
| Profile / Balanced | ContinuityAware | Support | 1.5249 | 0.0660 | 1.5698 | 28.78 | 0.11 | 1 |

## Provenance policy profile comparison

| policy profile | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| General | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.77 |
| General | file | 1.00 | 1.00 | 1.00 | 1.00 | 2.99 |
| Support | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.77 |
| Support | file | 1.00 | 1.00 | 1.00 | 1.00 | 2.99 |
| Research | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.78 |
| Research | file | 1.00 | 1.00 | 1.00 | 1.00 | 2.99 |
| Assistant | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.78 |
| Assistant | file | 1.00 | 1.00 | 1.00 | 1.00 | 2.99 |
| AutonomousAgent | sled | 1.00 | 1.00 | 1.00 | 1.00 | 2.76 |
| AutonomousAgent | file | 1.00 | 1.00 | 1.00 | 1.00 | 2.98 |

## Lifecycle maintenance timings

| backend | records | consolidation rec/s | consolidation mean ms | recall-during-maintenance p95 ms | integrity mean ms | repair mean ms | recovery import mean ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sled | 29 | 6779.63 | 4.28 | 2.83 | 2.63 | 3.67 | 3.15 |
| file | 29 | 6208.60 | 4.67 | 3.03 | 3.14 | 5.06 | 3.01 |
