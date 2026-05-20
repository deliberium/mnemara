# Mnemara Deployment

`mnemara-server` runs the sled-backed daemon and exposes gRPC plus HTTP/JSON admin surfaces.

## Start the daemon

```bash
cargo run -p mnemara-server
```

## Deployment profiles

`MNEMARA_DEPLOYMENT_PROFILE` selects the transport posture:

| Profile        | Purpose                             |
| -------------- | ----------------------------------- |
| `default`      | plain TCP gRPC plus HTTP            |
| `uds-local`    | local gRPC over Unix domain sockets |
| `tls-service`  | gRPC with TLS server identity       |
| `mtls-service` | gRPC with mutual TLS                |

Related transport variables:

- `MNEMARA_BIND_ADDR` for the gRPC TCP listener
- `MNEMARA_HTTP_BIND_ADDR` for the HTTP listener, or unset to disable HTTP
- `MNEMARA_GRPC_UDS_PATH` for the UDS socket path
- `MNEMARA_TLS_CERT_PATH` and `MNEMARA_TLS_KEY_PATH` for TLS identity
- `MNEMARA_TLS_CLIENT_CA_PATH` for trusted client CA material in `tls-service` and `mtls-service`

Example local UDS deployment:

```bash
MNEMARA_DEPLOYMENT_PROFILE=uds-local \
MNEMARA_GRPC_UDS_PATH=/tmp/mnemara.sock \
MNEMARA_HTTP_BIND_ADDR=127.0.0.1:50052 \
cargo run -p mnemara-server
```

Example TLS service deployment:

```bash
MNEMARA_DEPLOYMENT_PROFILE=tls-service \
MNEMARA_BIND_ADDR=0.0.0.0:50051 \
MNEMARA_TLS_CERT_PATH=./certs/server.pem \
MNEMARA_TLS_KEY_PATH=./certs/server-key.pem \
MNEMARA_TLS_CLIENT_CA_PATH=./certs/clients-ca.pem \
cargo run -p mnemara-server
```

For mTLS, point `MNEMARA_TLS_CLIENT_CA_PATH` at the CA used to issue client certificates and switch the profile to `mtls-service`.

Local certificate generation example:

```bash
mkdir -p certs
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout certs/server-key.pem \
  -out certs/server.pem \
  -days 365 \
  -subj "/CN=localhost"
chmod 600 certs/server-key.pem
```

For shared systems, keep private keys readable only by the service account and make the UDS path parent directory writable only by the processes that should connect.

## Runtime limits and fairness controls

The daemon enforces body/query limits plus bounded admission control before the store sees a request.

Key knobs:

- `MNEMARA_MAX_HTTP_BODY_BYTES`
- `MNEMARA_MAX_BATCH_UPSERT_REQUESTS`
- `MNEMARA_MAX_RECALL_ITEMS`
- `MNEMARA_MAX_QUERY_TEXT_BYTES`
- `MNEMARA_MAX_RECORD_CONTENT_BYTES`
- `MNEMARA_MAX_LABELS_PER_SCOPE`
- `MNEMARA_MAX_INFLIGHT_READS`
- `MNEMARA_MAX_INFLIGHT_WRITES`
- `MNEMARA_MAX_INFLIGHT_ADMIN`
- `MNEMARA_MAX_QUEUED_REQUESTS`
- `MNEMARA_MAX_TENANT_INFLIGHT`
- `MNEMARA_QUEUE_WAIT_TIMEOUT_MS`
- `MNEMARA_TRACE_RETENTION`

Admission fairness is currently FIFO per class with separate read, write, and admin budgets plus per-tenant inflight caps. Runtime status exposes queue depth, per-class wait ages, mean/max wait, and retained trace counts.

## Tuning and auth

Engine/runtime tuning:

- `MNEMARA_RECALL_SCORER_KIND`
- `MNEMARA_RECALL_SCORING_PROFILE`
- `MNEMARA_RECALL_PLANNING_PROFILE`
- `MNEMARA_RECALL_POLICY_PROFILE`
- `MNEMARA_GRAPH_EXPANSION_MAX_HOPS`
- `MNEMARA_EMBEDDING_PROVIDER_KIND`
- `MNEMARA_EMBEDDING_DIMENSIONS`
- `MNEMARA_COMPACTION_SUMMARIZE_AFTER_RECORD_COUNT`
- `MNEMARA_COMPACTION_COLD_ARCHIVE_AFTER_DAYS`
- `MNEMARA_COMPACTION_COLD_ARCHIVE_IMPORTANCE_THRESHOLD_PER_MILLE`

Planner tuning is additive. `MNEMARA_RECALL_PLANNING_PROFILE=fast_path`
preserves the lowest-latency default path, while
`MNEMARA_RECALL_PLANNING_PROFILE=continuity_aware` enables bounded continuity
expansion and richer planner traces. `MNEMARA_GRAPH_EXPANSION_MAX_HOPS`
controls the maximum graph-style expansion depth used by that planner profile.

`MNEMARA_RECALL_POLICY_PROFILE` controls provenance and lifecycle weighting for
different workloads. Supported values are `general`, `support`, `research`,
`assistant`, and `autonomous_agent`. `support` biases toward current,
high-trust records; `research` is more tolerant of historical context;
`assistant` balances continuity and verification; `autonomous_agent` keeps a
stricter provenance posture for execution-oriented workflows.

For embedded Rust integrations that need a custom semantic provider, there is
now a supported additive seam outside the environment-variable path. Use the
shared-embedder constructors in `mnemara-core` and supply your own provider note
string so explanation traces continue to disclose which provider produced the
semantic channel.

Auth:

- `MNEMARA_AUTH_TOKEN` for a single full-access bearer token
- `MNEMARA_AUTH_TOKENS` for role-scoped tokens in `token=perm1,perm2` form
- `MNEMARA_AUTH_PROTECT_METRICS=true` to require a metrics-scoped token for `/metrics`

Supported permissions:

- `read`
- `write`
- `admin`
- `metrics`

`/healthz` and `/readyz` stay unauthenticated.

## Admin endpoints

HTTP surfaces:

- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `POST /memory/upsert`
- `POST /memory/batch-upsert`
- `POST /memory/recall`
- `GET /admin/snapshot`
- `GET /admin/stats`
- `POST /admin/graph`
- `GET /admin/integrity`
- `POST /admin/repair`
- `POST /admin/maintenance/run`
- `POST /admin/compact`
- `POST /admin/delete`
- `GET /admin/traces`
- `GET /admin/traces/{trace_id}`
- `GET /admin/runtime`
- `POST /admin/export`
- `POST /admin/import`
- `POST /admin/replication/ship`

Trace listing supports `tenant_id`, `namespace`, `operation`, `status`, `before_started_at_unix_ms`, and `limit`.

`POST /admin/graph` returns a read-only graph inspection report for operator
tooling. Scope it with `tenant_id`, `namespace`, `actor_id`,
`conversation_id`, and `session_id`; archived, suppressed, and deleted records
remain hidden unless their explicit include flags are set.

Portable import supports:

- `Validate` mode for schema/data validation only
- `Merge` mode for non-destructive import
- `Replace` mode for full replacement
- `dry_run=true` for no-write validation previews

Import reports now disclose package compatibility, validated/imported/skipped counts, and structured failures.

`POST /admin/maintenance/run` orchestrates integrity checks, idempotency-key
repair, and tenant-scoped compaction in one admin operation. Its JSON payload
matches the individual operation scopes and supports `dry_run`, `reason`,
`run_integrity_check`, `run_repair`, `run_compaction`,
`remove_stale_idempotency_keys`, and `rebuild_missing_idempotency_keys`.

`POST /admin/replication/ship` exports a portable package from the local daemon
and posts it to a remote daemon's `/admin/import` endpoint. Snapshot shipping
currently targets `http://` daemon URLs and preserves import semantics through
the `Validate`, `Merge`, `Replace`, and `dry_run` fields.

## Background maintenance

Background maintenance is disabled by default. Enable it only for deployments
where scheduled admin work is expected:

- `MNEMARA_BACKGROUND_MAINTENANCE_ENABLED=true`
- `MNEMARA_BACKGROUND_MAINTENANCE_INTERVAL_SECONDS=3600`
- `MNEMARA_BACKGROUND_MAINTENANCE_TENANT` and `MNEMARA_BACKGROUND_MAINTENANCE_NAMESPACE` to scope work
- `MNEMARA_BACKGROUND_MAINTENANCE_DRY_RUN=false` to apply repairs and compaction
- `MNEMARA_BACKGROUND_MAINTENANCE_INTEGRITY`, `MNEMARA_BACKGROUND_MAINTENANCE_REPAIR`, and `MNEMARA_BACKGROUND_MAINTENANCE_COMPACTION` to enable or disable phases
- `MNEMARA_BACKGROUND_MAINTENANCE_REMOVE_STALE_IDEMPOTENCY_KEYS` and `MNEMARA_BACKGROUND_MAINTENANCE_REBUILD_MISSING_IDEMPOTENCY_KEYS` for repair behavior

## Lifecycle-aware operations

Compaction and retention now preserve more than archive state alone.

Operationally:

- compaction can emit summary records with lineage links back to source records
- duplicate consolidation can mark records as `superseded` instead of treating
  them as opaque archived entries
- cold archival and namespace-cap enforcement can preserve records as
  `historical` context rather than making them disappear from every recall view

The lifecycle counters exposed through compact and stats surfaces now include:

- superseded-record counts
- lineage-link counts
- historical-record counts

If you want admin or operator tooling to inspect archived material after a
maintenance pass, query with historical recall enabled rather than assuming
archived records are part of the default current-only view.

## Retrieval guidance for operators

The daemon now carries richer recall filters and explanations over both gRPC
and HTTP/JSON.

Useful lifecycle and continuity controls include:

- `episode_id` to constrain results to one episodic thread
- `continuity_states` and `unresolved_only` for open-loop style recall
- `historical_mode` to choose current-only, mixed, or historical-only recall
- `lineage_record_id` to inspect a derived record and its related lineage
- planning trace fields that expose planner stage, candidate sources, graph
  relation reasons, and the effective planning profile

For shared systems, start with the fast-path planner, then enable
continuity-aware retrieval only for workloads that benefit from episode and
lineage-sensitive recall.

Protocol compatibility stays additive for these retrieval controls. Older HTTP
or gRPC callers can omit episodic fields and the daemon will keep record-only
behavior. Missing additive lifecycle fields deserialize to safe defaults, and
portable JSON packages ignore unknown future additive fields during import.

For the complete release-candidate validation and fallback posture used before
promoting these capabilities, see `docs/release-validation.md`.
