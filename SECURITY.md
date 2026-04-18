# Security Policy

## Scope

Mnemara is a local-first Rust workspace for embedded and service-based AI memory systems. Security-sensitive areas include:

- daemon authentication and authorization
- gRPC and HTTP transport handling, including TLS, mTLS, and Unix domain socket deployment
- tenant, namespace, and record-isolation boundaries
- persistence, deletion, retention, compaction, and repair behavior in the file and sled backends
- portable export and import packages, including validation and replace flows
- benchmark, SDK, and admin surfaces that expose operational metadata

## Reporting a Vulnerability

Please do not disclose security issues publicly before maintainers have had a chance to assess them.

Use GitHub's private vulnerability reporting flow for this repository if it is available. If there is no private reporting channel enabled yet, contact the maintainers privately before opening a public issue.

When reporting an issue, include:

- the affected component or file paths
- a clear description of the impact
- reproduction steps or a proof of concept when safe to share
- whether the issue involves auth bypass, tenant isolation failure, secret exposure, arbitrary code execution, data corruption, or destructive import/delete behavior
- any mitigations, constraints, or configuration assumptions you already identified

## What to Report

Please report issues involving:

- authentication or authorization bypass in daemon or admin surfaces
- cross-tenant, cross-namespace, or cross-session data exposure
- insecure transport handling, certificate validation problems, or UDS permission mistakes that expand access unexpectedly
- import, export, delete, compaction, or repair behavior that can corrupt data or violate isolation guarantees
- idempotency, retention, or tombstone handling that can be abused to bypass expected safeguards
- secret exposure through logs, metrics, traces, or SDK behavior
- denial-of-service conditions that can exhaust memory, queue capacity, or storage unexpectedly
- dependency or supply-chain issues with practical security impact

## Disclosure Expectations

- Prefer coordinated disclosure.
- Avoid publishing proof-of-concept exploits before a fix or mitigation is available.
- Maintainers may ask for a reduced reproducer when a report cannot be verified directly.

## Supported Security Posture

This repository currently follows a best-effort security posture for the latest code on the default branch.

Security-relevant protections already present in the implementation include:

- bearer-token auth with role-scoped permissions for read, write, admin, and metrics access
- request limits, bounded admission control, and queue timeouts for the daemon
- explicit tenant, namespace, actor, conversation, and session scoping in the domain model
- retry-safe idempotency handling and structured import validation
- TLS, mTLS, and UDS transport modes for the gRPC server
- delete, integrity-check, and repair flows with explicit operator actions instead of silent recovery

Roadmap items should not be assumed to exist until they are documented in the main README and supporting docs.

## Hardening Guidance for Contributors

When changing security-sensitive code:

- add tests for failure paths and boundary conditions, not only success paths
- preserve or improve observability without logging secrets or sensitive record content unnecessarily
- document any new runtime assumptions, permissions, or transport requirements
- update public docs when the security posture changes materially
