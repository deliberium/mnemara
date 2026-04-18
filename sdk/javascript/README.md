# Mnemara JavaScript HTTP SDK

This package is the reference non-Rust SDK for Mnemara's HTTP API.

It is intentionally small:

- no runtime dependencies
- works with the standard `fetch` API in Node 18+ and modern browsers
- targets the daemon's HTTP endpoints for ingest, recall, stats, integrity checks, repair, compaction, delete, trace lookup, runtime status, export/import, health, readiness, and metrics

## Example

```js
import {
  MnemaraHttpClient,
  MemoryQualityState,
  MemoryRecordKind,
  MemoryTrustLevel,
} from "@mnemara/http-sdk";

const client = new MnemaraHttpClient({
  baseUrl: "http://127.0.0.1:50052",
  token: process.env.MNEMARA_TOKEN,
});

await client.upsert({
  idempotency_key: "interaction-42",
  record: {
    id: "interaction-42",
    scope: {
      tenant_id: "default",
      namespace: "conversation",
      actor_id: "ava",
      conversation_id: "thread-a",
      session_id: "session-a",
      source: "sdk-example",
      labels: ["demo"],
      trust_level: MemoryTrustLevel.Verified,
    },
    kind: MemoryRecordKind.Episodic,
    content: "Prompt: where is the repair command?\nAnswer: POST /admin/repair",
    summary: "repair command reference",
    source_id: null,
    metadata: {},
    quality_state: MemoryQualityState.Active,
    created_at_unix_ms: Date.now(),
    updated_at_unix_ms: Date.now(),
    expires_at_unix_ms: null,
    importance_score: 0.8,
    artifact: null,
  },
});

const recall = await client.recall({
  scope: {
    tenant_id: "default",
    namespace: "conversation",
    actor_id: "ava",
    conversation_id: "thread-a",
    session_id: "session-a",
    source: "sdk-example",
    labels: ["demo"],
    trust_level: MemoryTrustLevel.Verified,
  },
  query_text: "repair",
  max_items: 5,
  token_budget: null,
  include_explanation: true,
  filters: {
    kinds: [],
    required_labels: [],
    source: null,
    from_unix_ms: null,
    to_unix_ms: null,
    min_importance_score: null,
    trust_levels: [],
    states: [],
    include_archived: false,
  },
});

console.log(recall.hits[0]?.record.summary);
console.log(
  recall.explanation?.scorer_kind,
  recall.explanation?.scoring_profile,
);
console.log(recall.explanation?.selected_channels);
const snapshot = await client.snapshot();
const stats = await client.stats({ tenant_id: "default" });
console.log(snapshot.engine, stats.engine);
console.log(await client.integrityCheck({ tenant_id: "default" }));
console.log(await client.listTraces({ tenant_id: "default", limit: 5 }));
console.log(await client.runtimeStatus());

const portablePackage = await client.export({
  tenant_id: "default",
  include_archived: false,
});
console.log(
  await client.import({
    package: portablePackage,
    mode: "Validate",
    dry_run: true,
  }),
);
```

Example recall explanation payload shape:

```js
{
  selected_channels: ["lexical", "semantic", "policy"],
  policy_notes: [
    "initial_file_backend_scoring",
    "scoring_profile=balanced",
    "embedding_provider=deterministic_local",
  ],
  scorer_kind: "Profile",
  scoring_profile: "Balanced",
}
```

Example engine tuning payload shape from `snapshot()` or `stats()`:

```js
{
  recall_scorer_kind: "Profile",
  recall_scoring_profile: "Balanced",
  embedding_provider_kind: "DeterministicLocal",
  embedding_dimensions: 64,
  compaction_summarize_after_record_count: 50,
  compaction_cold_archive_after_days: 0,
  compaction_cold_archive_importance_threshold_per_mille: 250,
}
```

## Notes

- Authorization uses the daemon's `Authorization: Bearer <token>` flow.
- Enum values follow the daemon's JSON wire format, for example `"Episodic"` and `"Verified"`.
- Snapshot and stats responses include `engine` tuning metadata, including scorer and embedding configuration.
- Recall responses can include full planning traces and a `trace_id` that links directly to `/admin/traces`.
- Trace APIs expose backend, admission class, correlation ID, planning trace ID, and request summary metadata.
- Portable export/import flows support `Validate`, `Merge`, and `Replace`, plus `dry_run` previews and structured import failures.
- Runtime status surfaces queue depth, per-class inflight usage, per-tenant inflight counts, wait timing, and trace retention state.
- This package is kept in-repo as a reference SDK and can be published to a registry as part of a release process.
