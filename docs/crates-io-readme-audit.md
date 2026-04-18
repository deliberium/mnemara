# Crates.io README Audit

## Checklist

- [x] Add a reusable release checklist script
- [x] Audit the published crate landing-page strategy
- [x] Record crate-by-crate README recommendations for crates.io

## Summary

The shared workspace README is appropriate for the top-level `mnemara` facade crate because that crate is the primary entry point and benefits from the broader product overview.

The other published crates should switch to crate-specific READMEs before or alongside first publication. Their crates.io audiences are narrower, and the shared README spends too much space on unrelated workspace surfaces such as the JavaScript SDK, benchmark artifacts, and cross-crate deployment guidance.

## Recommendations

| Crate                | Keep shared README? | Recommendation                                                                                                                                                                           |
| -------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mnemara`            | Yes, for now        | The facade crate is the public entry point, so the workspace README is a reasonable fit. Long term, a facade-specific README would still be tighter, but it is not blocking publication. |
| `mnemara-core`       | No                  | Switch to a crate-specific README focused on the core domain model, traits, scoring, evaluation helpers, and portable types.                                                             |
| `mnemara-store-file` | No                  | Switch to a crate-specific README focused on the file-backed store, local durability model, and compatibility use cases.                                                                 |
| `mnemara-store-sled` | No                  | Switch to a crate-specific README focused on the sled backend, embedded deployment, and operational tradeoffs.                                                                           |
| `mnemara-protocol`   | No                  | Switch to a crate-specific README focused on protobuf and gRPC generated types, schema usage, and client/server integration.                                                             |
| `mnemara-server`     | No                  | Switch to a crate-specific README focused on the daemon binary, install commands, environment variables, and deployment profiles.                                                        |

## Why the split is worth it

Crates.io pages work best when the first screen answers three questions quickly:

1. What does this crate do?
2. How do I add or install it?
3. What is the smallest working example?

The shared workspace README answers those questions well for `mnemara`, but it is too broad for the backend, protocol, and server crates.

## Proposed next step

Keep the shared README for `mnemara` during the initial publish sequence.

Crate-specific `README.md` files are now in place for:

- `crates/mnemara-core`
- `crates/mnemara-store-file`
- `crates/mnemara-store-sled`
- `crates/mnemara-protocol`
- `crates/mnemara-server`

The next release should publish those crate-specific landing pages and keep the shared workspace README only for the facade crate `mnemara`.
