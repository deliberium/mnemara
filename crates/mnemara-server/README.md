# mnemara-server

`mnemara-server` provides the standalone Mnemara daemon plus the reusable Axum and tonic service surface used to expose the memory engine over HTTP and gRPC.

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

## Notes

- uses `mnemara-store-sled` as the backing store
- exposes health, readiness, memory, admin, metrics, and trace endpoints
- supports TCP, Unix domain sockets, TLS, and mutual TLS deployment profiles

Deployment guide: <https://github.com/deliberium/mnemara/blob/main/docs/deployment.md>
