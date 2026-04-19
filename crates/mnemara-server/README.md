# mnemara-server

`mnemara-server` provides the standalone Mnemara daemon plus the reusable Axum and tonic service surface used to expose the memory engine over HTTP and gRPC.

Choose this crate when you want the packaged daemon binary or you need to embed the reference Axum and tonic service surfaces inside another Rust service.

## Install

Install the published daemon binary with:

```bash
cargo install mnemara-server
```

If you want to embed the server crate inside another Rust application instead of installing the binary, add it as a dependency with:

```bash
cargo add mnemara-server
```

## Run

Start the daemon with:

```bash
mnemara-server
```

Useful environment variables include:

- `MNEMARA_BIND_ADDR` for the gRPC listen address
- `MNEMARA_HTTP_BIND_ADDR` for the HTTP listen address
- `MNEMARA_DATA_DIR` for the sled data directory
- `MNEMARA_DEPLOYMENT_PROFILE` for `default`, `uds-local`, `tls-service`, or `mtls-service`
- `MNEMARA_AUTH_TOKEN`, `MNEMARA_AUTH_TOKENS`, and `MNEMARA_AUTH_PROTECT_METRICS` for bearer-token auth policy
- `MNEMARA_MAX_*` limits and `MNEMARA_TRACE_RETENTION` for request sizing, admission, and trace retention controls
- `MNEMARA_RECALL_*` and embedding-related environment variables for scorer, planning, policy, and semantic-provider tuning

## Notes

- uses `mnemara-store-sled` as the backing store
- exposes health, readiness, memory, lifecycle, admin, metrics, runtime-status, and trace endpoints over HTTP and gRPC
- serves episodic memory records, continuity-aware recall filters, planning traces, lifecycle controls, portable import or export flows, and repair or integrity operations
- supports bearer-token auth with role-scoped read, write, admin, and metrics permissions
- includes bounded admission control, request limits, correlation IDs, and metrics or trace observability for daemon deployments
- supports TCP, Unix domain sockets, TLS, and mutual TLS deployment profiles

Deployment guide: <https://github.com/deliberium/mnemara/blob/master/docs/deployment.md>
