# ADR 0001: Ranking defaults and benchmark publication

## Status

Accepted.

## Context

The roadmap required Mnemara to publish reproducible evaluation artifacts, compare scorer families and profiles, and make ranking trade-offs explicit before first release.

The checked-in benchmark artifacts in `docs/benchmark-artifacts/benchmark-report-v1.{json,md}` show:

- `Profile / Balanced` and `Curated / Balanced` both achieved perfect quality on the current ranked corpus for both backends
- `Profile / LexicalFirst` regressed MRR and NDCG slightly even though Hit@3 and Recall@3 stayed perfect
- `Curated / ImportanceFirst` stayed strong but did not improve the published corpus over the balanced defaults

## Decision

Mnemara keeps `Profile / Balanced` as the default release posture.

`Curated` remains a first-class supported option for applications that want stronger trust and quality-state promotion, but it is not promoted to the default without broader corpus evidence that it improves real workloads enough to justify the extra policy bias.

The published benchmark runner and workflow are now part of the release evidence. Ranking changes should update:

- `data/evaluation/ranking-corpus-v1.json` when the judged corpus changes
- `docs/benchmark-artifacts/benchmark-report-v1.{json,md}` after rerunning the generator
- this ADR set when defaults or trade-offs change

## Consequences

- Default behavior remains predictable and benchmark-backed.
- Operators can compare backends and profiles from checked-in artifacts instead of relying on anecdotal tuning.
- Future ranking changes have an explicit place to document why a default changed.
