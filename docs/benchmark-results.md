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

| Field        | Value     |
| ------------ | --------- |
| OS           | `macos`   |
| Architecture | `aarch64` |
| Logical CPUs | `10`      |

## Quality summary

Across the published 18-case ranked corpus, the comparison run produced:

| Scorer / profile          | Planner profile | Backend |  Hit@3 | Recall@3 |    MRR | NDCG@3 |
| ------------------------- | --------------- | ------- | -----: | -------: | -----: | -----: |
| Profile / Balanced        | FastPath        | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |
| Profile / Balanced        | FastPath        | file    | `1.00` |   `1.00` | `1.00` | `1.00` |
| Profile / Balanced        | ContinuityAware | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |
| Profile / Balanced        | ContinuityAware | file    | `1.00` |   `1.00` | `1.00` | `1.00` |
| Profile / LexicalFirst    | FastPath        | sled    | `1.00` |   `1.00` | `0.97` | `0.98` |
| Profile / LexicalFirst    | FastPath        | file    | `1.00` |   `1.00` | `0.97` | `0.98` |
| Curated / Balanced        | FastPath        | sled    | `1.00` |   `1.00` | `1.00` | `0.99` |
| Curated / Balanced        | FastPath        | file    | `1.00` |   `1.00` | `1.00` | `0.99` |
| Curated / ImportanceFirst | FastPath        | sled    | `1.00` |   `1.00` | `1.00` | `0.99` |
| Curated / ImportanceFirst | FastPath        | file    | `1.00` |   `1.00` | `1.00` | `0.99` |

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
- chronology reconstruction
- recurrence pattern
- duration boundary
- continuity unresolved
- contradiction handling
- preference change
- operational drift
- long-horizon task

## Performance summary

Headline latency figures from `benchmark-report-v1.md`:

| Scorer / profile          | Planner profile | Backend | Ingest mean ms | Recall p95 ms | Import mean ms |
| ------------------------- | --------------- | ------- | -------------: | ------------: | -------------: |
| Profile / Balanced        | FastPath        | sled    |       `168.05` |        `1.31` |        `13.12` |
| Profile / Balanced        | FastPath        | file    |        `42.82` |        `1.31` |        `13.37` |
| Profile / Balanced        | ContinuityAware | sled    |       `180.58` |        `1.33` |        `15.34` |
| Profile / Balanced        | ContinuityAware | file    |        `48.22` |        `1.31` |         `8.95` |
| Profile / LexicalFirst    | FastPath        | sled    |       `204.96` |        `1.31` |        `15.54` |
| Profile / LexicalFirst    | FastPath        | file    |        `39.03` |        `1.35` |         `7.98` |
| Curated / Balanced        | FastPath        | sled    |       `169.38` |        `1.31` |        `13.48` |
| Curated / Balanced        | FastPath        | file    |        `44.22` |        `1.37` |         `5.50` |
| Curated / ImportanceFirst | FastPath        | sled    |       `172.13` |        `1.34` |        `14.69` |
| Curated / ImportanceFirst | FastPath        | file    |        `41.44` |        `1.36` |         `5.07` |

The JSON artifact also contains:

- ingest throughput per second
- recall mean, p50, p95, and max
- snapshot, stats, export, dry-run compaction, and replace-import timings
- exported storage-byte totals

## Salience-isolated comparison

The current artifact revision now publishes a salience-isolated slice with the
same `Profile / Balanced / ContinuityAware / General` configuration run twice:
once with authored episodic salience intact and once with episodic salience
neutralized to defaults on the same corpus.

That slice exists to keep the roadmap salience claim honest. It isolates the
quality and latency effect of episodic salience without changing planner
profile, policy profile, backend, or the judged corpus itself.

On the current 18-case corpus, quality stayed flat at `1.00` for Hit@3,
Recall@3, MRR, and NDCG@3 in both conditions. Recall p95 moved from `1.28` ms
to `1.35` ms on sled when salience was neutralized, and from `1.36` ms to
`1.32` ms on file, so the present checked-in corpus shows a measurable but
small latency delta and no quality separation yet.

## Planner stage timings

The current artifact revision now isolates planner-stage timings directly from
the core planner implementation.

| Scorer / profile   | Planner profile | Policy profile | Candidate mean ms | Graph p95 ms | Total mean ms | Mean seeded | Mean expanded | Max hops |
| ------------------ | --------------- | -------------- | ----------------: | -----------: | ------------: | ----------: | ------------: | -------: |
| Profile / Balanced | FastPath        | General        |          `0.4888` |     `0.0112` |      `0.4981` |     `26.88` |        `0.00` |      `0` |
| Profile / Balanced | ContinuityAware | General        |          `0.4838` |     `0.0280` |      `0.5040` |     `26.88` |        `0.12` |      `1` |
| Profile / Balanced | ContinuityAware | Support        |          `0.4857` |     `0.0281` |      `0.5059` |     `26.88` |        `0.12` |      `1` |

This makes the current public latency claim more precise: continuity-aware
planning still increases total planning cost only slightly on the shipped
corpus, and the graph-expansion stage remains tightly bounded under the default
hop cap.

## Provenance policy comparison

The published artifact also now isolates provenance-policy comparisons while
keeping semantic mode fixed at `DeterministicLocal`.

| Policy profile  | Backend |  Hit@3 | Recall@3 |    MRR | NDCG@3 | Recall p95 ms |
| --------------- | ------- | -----: | -------: | -----: | -----: | ------------: |
| General         | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.32` |
| General         | file    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.37` |
| Support         | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.29` |
| Support         | file    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.31` |
| Research        | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.20` |
| Research        | file    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.33` |
| Assistant       | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.31` |
| Assistant       | file    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.28` |
| AutonomousAgent | sled    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.36` |
| AutonomousAgent | file    | `1.00` |   `1.00` | `1.00` | `1.00` |        `1.32` |

The corpus does not currently separate the profiles on quality metrics, but it
does now publish their latency posture independently from semantic enablement.

## Lifecycle maintenance timings

The current artifact revision now publishes maintenance costs directly for the
fixed 27-record corpus instead of requiring inference from generic admin
operations.

| Backend | Records | Consolidation rec/s | Consolidation mean ms | Recall-during-maintenance p95 ms | Integrity mean ms | Repair mean ms | Recovery import mean ms |
| ------- | ------: | ------------------: | --------------------: | -------------------------------: | ----------------: | -------------: | ----------------------: |
| sled    |    `27` |           `2598.05` |               `10.39` |                           `1.77` |            `2.69` |         `8.21` |                 `12.58` |
| file    |    `27` |           `3800.32` |                `7.10` |                           `1.45` |            `2.06` |         `3.93` |                 `17.25` |

This closes the previous evidence gap for lifecycle maintenance posture:
consolidation throughput is now published on the fixed corpus, recall latency is
measured while dry-run maintenance work is running, and both integrity-repair
and import-based recovery timings are published alongside the qualitative
scenario tables.

## Portability and admin status

The release evidence now includes:

- validate-only import reports with no writes applied
- dry-run import reports with structured failures
- package-version compatibility reporting
- file-to-sled roundtrip coverage
- admin trace filtering plus runtime fairness/retention status

## Published roadmap-era benchmark evidence

The checked-in `benchmark-report-v1.*` artifacts now include the expanded
episodic, continuity-aware, and lifecycle-sensitive corpus slices.

That published artifact revision is complemented by checked-in tests for
explanation fidelity, transport behavior, and lifecycle semantics.

Current repository evidence includes:

- the expanded ranked corpus in `data/evaluation/ranking-corpus-v1.json`, including chronology, unresolved continuity, contradiction, preference-change, drift, and long-horizon task slices
- the expanded ranked corpus in `data/evaluation/ranking-corpus-v1.json`, including chronology, recurrence-pattern, duration-boundary, unresolved continuity, contradiction, preference-change, drift, and long-horizon task slices
- published planner-profile and scenario-stratified benchmark artifacts in `docs/benchmark-artifacts/benchmark-report-v1.json` and `docs/benchmark-artifacts/benchmark-report-v1.md`
- published salience-enabled versus salience-neutralized quality and recall latency comparisons in `docs/benchmark-artifacts/benchmark-report-v1.json` and `docs/benchmark-artifacts/benchmark-report-v1.md`
- published planner-stage timing slices for candidate generation, graph expansion, and total planning in `docs/benchmark-artifacts/benchmark-report-v1.json` and `docs/benchmark-artifacts/benchmark-report-v1.md`
- published provenance-policy comparisons with semantic mode held constant in `docs/benchmark-artifacts/benchmark-report-v1.json` and `docs/benchmark-artifacts/benchmark-report-v1.md`
- published lifecycle maintenance timings for consolidation throughput, recall during maintenance, integrity checks, repair rebuilds, and import-based recovery in `docs/benchmark-artifacts/benchmark-report-v1.json` and `docs/benchmark-artifacts/benchmark-report-v1.md`
- episodic and salience score reporting in `mnemara-core` unit tests
- continuity-aware planner expansion and fast-path regression coverage in `mnemara-core`
- bounded multi-hop hop-limit and scope-boundary coverage in `mnemara-core`
- planner-stage, candidate-source, and planning-profile transport coverage in `mnemara-server/tests/service_roundtrip.rs`
- golden explanation payload and user-guide query regressions in `mnemara-server/tests/rollout_examples.rs`
- lifecycle-aware archival, supersession, historical recall, and restart-safe repair behavior in both backend replay suites
- the benchmark runner implementation in `crates/mnemara-store-sled/examples/publish_benchmarks.rs`
- a full serial workspace validation run via:

```bash
cargo test --manifest-path /Users/kabudu/projex/deliberium-group/mnemara/Cargo.toml --workspace -- --test-threads=1
```

That means the current public claim boundary is:

- shipped: episodic fields, planner traces, historical recall controls, lineage-preserving lifecycle behavior
- published in checked-in benchmark artifacts: planner-profile latency comparison, salience-isolated quality and latency comparison, planner-stage timing slices, provenance-policy comparisons, lifecycle-specific quantitative scenario tables, and lifecycle maintenance timings
- still validated separately by focused suites: explanation payload stability, transport trace behavior, backend lifecycle parity, and restart-safe repair behavior
