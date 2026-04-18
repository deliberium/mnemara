# mnemara-protocol

`mnemara-protocol` publishes the protobuf and gRPC types used by the Mnemara service surface.

## Install

Add the crate to your Rust project with:

```bash
cargo add mnemara-protocol
```

## What it exposes

- generated protobuf messages under `mnemara_protocol::v1`
- generated gRPC client and server traits for the Mnemara memory service

## Minimal example

```rust
use mnemara_protocol::v1::RecallRequest;

let request = RecallRequest {
    query_text: "reconnect storm mitigation".to_string(),
    ..Default::default()
};
```

## Notes

- intended for Rust gRPC clients and servers that need the wire-level schema
- pairs with `mnemara-server` for the reference daemon implementation
- embedded applications that only need the domain model should depend on `mnemara-core` or the `mnemara` facade instead

Project documentation: <https://github.com/deliberium/mnemara>
