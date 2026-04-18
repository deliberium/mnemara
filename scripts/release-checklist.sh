#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
manifest_path="${MNEMARA_MANIFEST_PATH:-$repo_root/Cargo.toml}"

phase="${1:-help}"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

print_help() {
  cat <<EOF
Usage: $(basename "$0") <phase>

Phases:
  preflight     Run fmt, clippy, and tests against the workspace manifest.
  foundation    Package crates that can verify before the internal dependency graph is published.
  storage       Package mnemara-store-file and mnemara-store-sled.
  server        Package mnemara-server.
  facade        Package the mnemara facade crate.
  dry-run-publish Run cargo publish --dry-run for a phase: foundation, storage, server, facade, or all.
  publish-plan  Print the recommended publish order and commands.
  help          Show this help.

Notes:
  - Use the workspace manifest at: $manifest_path
  - foundation can run before any crates are published.
  - storage, server, and facade require earlier crates to already exist on crates.io for Cargo verification.
EOF
}

preflight() {
  run cargo fmt --manifest-path "$manifest_path" --all --check
  run cargo clippy --manifest-path "$manifest_path" --workspace --all-targets
  run cargo test --manifest-path "$manifest_path" --workspace
}

package_crate() {
  local crate_name="$1"
  run cargo package --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty
}

dry_run_publish_crate() {
  local crate_name="$1"
  run cargo publish --dry-run --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty
}

foundation() {
  package_crate mnemara-core
  package_crate mnemara-protocol
}

storage() {
  package_crate mnemara-store-file
  package_crate mnemara-store-sled
}

server() {
  package_crate mnemara-server
}

facade() {
  package_crate mnemara
}

dry_run_publish() {
  local target="${2:-all}"
  case "$target" in
    foundation)
      dry_run_publish_crate mnemara-core
      dry_run_publish_crate mnemara-protocol
      ;;
    storage)
      dry_run_publish_crate mnemara-store-file
      dry_run_publish_crate mnemara-store-sled
      ;;
    server)
      dry_run_publish_crate mnemara-server
      ;;
    facade)
      dry_run_publish_crate mnemara
      ;;
    all)
      dry_run_publish_crate mnemara-core
      dry_run_publish_crate mnemara-store-file
      dry_run_publish_crate mnemara-store-sled
      dry_run_publish_crate mnemara-protocol
      dry_run_publish_crate mnemara-server
      dry_run_publish_crate mnemara
      ;;
    *)
      printf 'Unknown dry-run-publish target: %s\n' "$target" >&2
      printf 'Expected one of: foundation, storage, server, facade, all\n' >&2
      exit 1
      ;;
  esac
}

publish_plan() {
  cat <<EOF
Recommended publish order:
  1. cargo publish --manifest-path "$manifest_path" -p mnemara-core
  2. cargo publish --manifest-path "$manifest_path" -p mnemara-store-file
  3. cargo publish --manifest-path "$manifest_path" -p mnemara-store-sled
  4. cargo publish --manifest-path "$manifest_path" -p mnemara-protocol
  5. cargo publish --manifest-path "$manifest_path" -p mnemara-server
  6. cargo publish --manifest-path "$manifest_path" -p mnemara

Recommended verification flow:
  $(basename "$0") preflight
  $(basename "$0") foundation
  $(basename "$0") dry-run-publish foundation
  # publish foundation crates first, then continue
  $(basename "$0") storage
  $(basename "$0") dry-run-publish storage
  $(basename "$0") server
  $(basename "$0") dry-run-publish server
  $(basename "$0") facade
  $(basename "$0") dry-run-publish facade
EOF
}

case "$phase" in
  preflight)
    preflight
    ;;
  foundation)
    foundation
    ;;
  storage)
    storage
    ;;
  server)
    server
    ;;
  facade)
    facade
    ;;
  dry-run-publish)
    dry_run_publish "$@"
    ;;
  publish-plan)
    publish_plan
    ;;
  help|-h|--help)
    print_help
    ;;
  *)
    printf 'Unknown phase: %s\n\n' "$phase" >&2
    print_help >&2
    exit 1
    ;;
esac