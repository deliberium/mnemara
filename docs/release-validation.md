# Mnemara Release Validation and Rollout Guide

This document defines the release-candidate gate for shipped roadmap work.

It exists to keep code, tests, benchmark claims, repository documentation, and
website messaging aligned before release notes or homepage copy promote a new
capability.

## Current capability boundary

The current shipped release boundary includes:

- episodic record fields and continuity-aware filtering
- recurrence, duration, and boundary cues in the episode contract and temporal scoring
- planner profiles with planner-stage and candidate-source traces
- lifecycle-aware historical recall, lineage links, and supersession-aware compaction
- embedded file and sled backends plus daemon transport parity for those fields

The current release boundary does not claim that contradiction handling,
preference drift resolution, or long-horizon narrative reasoning are complete
end-to-end products.

## Release-candidate gate

Before shipping a release candidate for roadmap-era retrieval or lifecycle
changes, run all of the following.

### Rust workspace validation

```bash
cargo fmt --manifest-path ./Cargo.toml --all --check
cargo clippy --manifest-path ./Cargo.toml --workspace --all-targets
cargo test --manifest-path ./Cargo.toml --workspace -- --test-threads=1
```

The serial test run is the primary regression gate for explanation fidelity,
backend parity, and lifecycle visibility semantics.

### Required focused suites

These are the minimum targeted checks for shipped episodic, planner, and
lifecycle work:

```bash
cargo test --manifest-path ./Cargo.toml -p mnemara-core --lib
cargo test --manifest-path ./Cargo.toml -p mnemara-core --test evaluation_corpus
cargo test --manifest-path ./Cargo.toml -p mnemara-server --test service_roundtrip
cargo test --manifest-path ./Cargo.toml -p mnemara-server --test rollout_examples
cargo test --manifest-path ./Cargo.toml -p mnemara-store-file --test replay_fixtures
cargo test --manifest-path ./Cargo.toml -p mnemara-store-sled --test replay_fixtures
```

Those focused suites are the minimum acceptance checks for:

- the checked-in ranked corpus slices for chronology, contradiction, drift, preference change, and long-horizon continuity
- golden explanation payload stability for shipped HTTP examples
- documented episode, unresolved-only, historical-only, and lineage-aware query patterns

### Website rollout validation

The standalone marketing site must build cleanly before release copy claims a
new capability.

From the sibling `mnemara-web/` project:

```bash
pnpm build
pnpm typecheck
```

### Documentation and evidence review

Confirm that these repository surfaces match the shipped behavior and do not
overclaim:

- `README.md`
- `docs/architecture.md`
- `docs/user-guide.md`
- `docs/deployment.md`
- `docs/benchmark-methodology.md`
- `docs/benchmark-results.md`
- `CHANGELOG.md`
- `../mnemara-web/public/index.html`

### Benchmark and claim review

Before adding quantitative claims, verify that either:

1. a checked-in benchmark artifact revision includes the new scenario slices and environment disclosure, or
2. the docs and website explicitly say the capability is shipped but currently evidenced by checked-in tests rather than a new benchmark artifact revision

## Delivery-mode matrix

Each release candidate should preserve parity across these delivery modes:

| Mode                | Required evidence                                                                          |
| ------------------- | ------------------------------------------------------------------------------------------ |
| Embedded file store | `mnemara-store-file` replay fixtures and portable workflows stay green                     |
| Embedded sled store | `mnemara-store-sled` replay fixtures, portable workflows, and benchmark runner stay green  |
| Daemon              | `mnemara-server` roundtrip, transport, admin trace, and runtime status coverage stay green |

## Fallback and feature-gating posture

The current milestone does not introduce dedicated runtime feature flags for the
episodic, planner, or lifecycle work. Rollout safety currently depends on
configuration fallback rather than hard capability gating.

Use these fallback positions when a deployment needs a safer posture:

- `MNEMARA_RECALL_PLANNING_PROFILE=fast_path` to disable continuity-aware expansion
- `MNEMARA_GRAPH_EXPANSION_MAX_HOPS=0` to force no graph-style expansion even if planner logic evolves
- `historical_mode=current_only` for current-state-biased recall in clients and operator tools
- transport posture fallback from TCP/TLS to `uds-local` for same-host deployments when exposure risk is a concern

If a future milestone introduces higher-risk behavior, add an explicit feature
flag or rollout toggle before broad release messaging.

## Promotion rule

Do not promote a capability in release notes or on the website unless:

- the implementation is merged
- the release-candidate gate above is green
- repository docs describe the capability and its limits
- the website separates shipped behavior from roadmap work
- quantitative claims are tied to checked-in benchmark artifacts or exact test commands
