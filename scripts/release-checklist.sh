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
  release-candidate Run the release-candidate validation gate, including serial tests and website verification.
  bump-version  Update the workspace and internal crate dependency versions across Cargo.toml files.
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
  - dry-run-publish all continues past expected publish-order gating failures and reports skipped crates.
EOF
}

bump_version() {
  local new_version="${2:-}"
  local cargo_tomls=()
  local manifest

  if [[ -z "$new_version" ]]; then
    printf 'Usage: %s bump-version <semver>\n' "$(basename "$0")" >&2
    exit 1
  fi

  if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    printf 'Invalid version: %s\n' "$new_version" >&2
    printf 'Expected a semver value like 0.2.0 or 0.2.0-rc.1\n' >&2
    exit 1
  fi

  while IFS= read -r manifest; do
    cargo_tomls+=("$manifest")
  done < <(find "$repo_root" \( -path "$repo_root/target" -o -path "$repo_root/.git" \) -prune -o -name Cargo.toml -type f -print | sort)

  if [[ ${#cargo_tomls[@]} -eq 0 ]]; then
    printf 'No Cargo.toml files found under %s\n' "$repo_root" >&2
    exit 1
  fi

  for manifest in "${cargo_tomls[@]}"; do
    run perl -0pi -e 's/^version = "[^"]+"$/version = "'"$new_version"'"/m' "$manifest"
    run perl -0pi -e 's/(mnemara(?:-[a-z0-9]+)*\s*=\s*\{[^\n]*version\s*=\s*")[^"]+("[^\n]*\})/${1}'"$new_version"'${2}/g' "$manifest"
  done
}

preflight() {
  run cargo fmt --manifest-path "$manifest_path" --all --check
  run cargo clippy --manifest-path "$manifest_path" --workspace --all-targets
  run cargo test --manifest-path "$manifest_path" --workspace
}

release_candidate() {
  local website_root="$repo_root/../mnemara-web"

  run cargo fmt --manifest-path "$manifest_path" --all --check
  run cargo clippy --manifest-path "$manifest_path" --workspace --all-targets
  run cargo test --manifest-path "$manifest_path" --workspace -- --test-threads=1

  if [[ -d "$website_root" ]]; then
    run test -f "$repo_root/docs/benchmark-methodology.md"
    run test -f "$repo_root/docs/benchmark-results.md"
    run test -f "$repo_root/docs/release-validation.md"
    run pnpm --dir "$website_root" build
    run pnpm --dir "$website_root" typecheck
  else
    printf '\n==> skipping website validation: %s not found\n' "$website_root"
  fi
}

package_crate() {
  local crate_name="$1"
  run cargo package --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty
}

dry_run_publish_crate() {
  local crate_name="$1"
  run cargo publish --dry-run --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty
}

dry_run_publish_crate_or_skip() {
  local crate_name="$1"
  shift
  local required_dependencies=("$@")
  local output_file
  local dependency

  output_file="$(mktemp)"
  printf '\n==> cargo publish --dry-run --manifest-path %s -p %s --allow-dirty\n' "$manifest_path" "$crate_name"
  if cargo publish --dry-run --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty >"$output_file" 2>&1; then
    cat "$output_file"
    rm -f "$output_file"
    return 0
  fi

  for dependency in "${required_dependencies[@]}"; do
    if grep -Fq "no matching package named \`$dependency\` found" "$output_file" \
      || { grep -Fq "failed to select a version for the requirement \`$dependency =" "$output_file" \
        && grep -Fq "candidate versions found which didn't match" "$output_file"; }; then
      printf '\n==> skipping %s: crates.io does not yet have required dependency %s\n' "$crate_name" "$dependency"
      rm -f "$output_file"
      return 0
    fi
  done

  cat "$output_file"
  rm -f "$output_file"
  return 1
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
      dry_run_publish_crate mnemara-protocol
      dry_run_publish_crate_or_skip mnemara-store-file mnemara-core
      dry_run_publish_crate_or_skip mnemara-store-sled mnemara-core
      dry_run_publish_crate_or_skip mnemara-server mnemara-core mnemara-protocol mnemara-store-sled
      dry_run_publish_crate_or_skip mnemara mnemara-core mnemara-store-file mnemara-store-sled mnemara-protocol mnemara-server
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
  2. cargo publish --manifest-path "$manifest_path" -p mnemara-protocol
  3. cargo publish --manifest-path "$manifest_path" -p mnemara-store-file
  4. cargo publish --manifest-path "$manifest_path" -p mnemara-store-sled
  5. cargo publish --manifest-path "$manifest_path" -p mnemara-server
  6. cargo publish --manifest-path "$manifest_path" -p mnemara

Recommended verification flow:
  $(basename "$0") preflight
  $(basename "$0") release-candidate
  $(basename "$0") bump-version 0.2.0
  $(basename "$0") foundation
  $(basename "$0") dry-run-publish foundation
  # publish mnemara-core and mnemara-protocol first, then continue
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
  release-candidate)
    release_candidate
    ;;
  bump-version)
    bump_version "$@"
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