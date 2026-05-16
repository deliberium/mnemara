#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
manifest_path="${MNEMARA_MANIFEST_PATH:-$repo_root/Cargo.toml}"
publish_attempts="${MNEMARA_PUBLISH_ATTEMPTS:-8}"
publish_initial_wait_seconds="${MNEMARA_PUBLISH_INITIAL_WAIT_SECONDS:-20}"
publish_max_wait_seconds="${MNEMARA_PUBLISH_MAX_WAIT_SECONDS:-300}"

phase="${1:-help}"

publish_order=(
  mnemara-core
  mnemara-protocol
  mnemara-store-file
  mnemara-store-sled
  mnemara-server
  mnemara
)

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
  publish       Build, verify, and publish crates to crates.io in dependency order.
  publish-plan  Print the recommended publish order and commands.
  help          Show this help.

Notes:
  - Use the workspace manifest at: $manifest_path
  - foundation can run before any crates are published.
  - storage, server, and facade require earlier crates to already exist on crates.io for Cargo verification.
  - dry-run-publish all continues past expected publish-order gating failures and reports skipped crates.
  - publish accepts a target: foundation, storage, server, facade, all, or one crate name.
  - publish retries crates.io 429/index propagation failures and skips versions that already exist.
  - Tune publish retry behavior with MNEMARA_PUBLISH_ATTEMPTS, MNEMARA_PUBLISH_INITIAL_WAIT_SECONDS, and MNEMARA_PUBLISH_MAX_WAIT_SECONDS.
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

crate_version() {
  local crate_name="$1"
  local crate_manifest

  crate_manifest="$(crate_manifest_path "$crate_name")"
  awk -F\" '/^version = / { print $2; exit }' "$crate_manifest"
}

crate_manifest_path() {
  local crate_name="$1"

  case "$crate_name" in
    mnemara-core)
      printf '%s\n' "$repo_root/crates/mnemara-core/Cargo.toml"
      ;;
    mnemara-protocol)
      printf '%s\n' "$repo_root/crates/mnemara-protocol/Cargo.toml"
      ;;
    mnemara-store-file)
      printf '%s\n' "$repo_root/crates/mnemara-store-file/Cargo.toml"
      ;;
    mnemara-store-sled)
      printf '%s\n' "$repo_root/crates/mnemara-store-sled/Cargo.toml"
      ;;
    mnemara-server)
      printf '%s\n' "$repo_root/crates/mnemara-server/Cargo.toml"
      ;;
    mnemara)
      printf '%s\n' "$repo_root/crates/mnemara/Cargo.toml"
      ;;
    *)
      printf 'Unknown crate: %s\n' "$crate_name" >&2
      return 1
      ;;
  esac
}

publish_targets() {
  local target="$1"

  case "$target" in
    foundation)
      printf '%s\n' mnemara-core mnemara-protocol
      ;;
    storage)
      printf '%s\n' mnemara-store-file mnemara-store-sled
      ;;
    server)
      printf '%s\n' mnemara-server
      ;;
    facade)
      printf '%s\n' mnemara
      ;;
    all)
      printf '%s\n' "${publish_order[@]}"
      ;;
    mnemara-core|mnemara-protocol|mnemara-store-file|mnemara-store-sled|mnemara-server|mnemara)
      printf '%s\n' "$target"
      ;;
    *)
      printf 'Unknown publish target: %s\n' "$target" >&2
      printf 'Expected one of: foundation, storage, server, facade, all, or a Mnemara crate name\n' >&2
      return 1
      ;;
  esac
}

output_indicates_already_published() {
  local output_file="$1"
  grep -Eiq 'already (uploaded|exists)|is already uploaded|crate version .* is already' "$output_file"
}

output_indicates_retryable_publish_failure() {
  local output_file="$1"

  grep -Eiq '(^|[^0-9])429([^0-9]|$)|too many requests|rate limit|try again later|Retry-After|failed to get a 200 OK response|failed to get successful HTTP response' "$output_file" \
    || grep -Eiq 'no matching package named `mnemara|failed to select a version for the requirement `mnemara|candidate versions found which didn'\''t match' "$output_file"
}

retry_after_seconds() {
  local output_file="$1"
  local retry_after

  retry_after="$(grep -Eio 'retry-after: *[0-9]+' "$output_file" | awk '{ print $2 }' | tail -n 1)"
  if [[ -n "$retry_after" ]]; then
    printf '%s\n' "$retry_after"
  fi
}

wait_before_publish_retry() {
  local output_file="$1"
  local attempt="$2"
  local wait_seconds
  local retry_after

  retry_after="$(retry_after_seconds "$output_file")"
  if [[ -n "$retry_after" ]]; then
    wait_seconds="$retry_after"
  else
    wait_seconds=$((publish_initial_wait_seconds * attempt))
  fi

  if (( wait_seconds > publish_max_wait_seconds )); then
    wait_seconds="$publish_max_wait_seconds"
  fi

  printf '\n==> crates.io is not ready yet; retrying in %s seconds\n' "$wait_seconds"
  sleep "$wait_seconds"
}

validate_publish_retry_config() {
  if [[ ! "$publish_attempts" =~ ^[1-9][0-9]*$ ]]; then
    printf 'Invalid MNEMARA_PUBLISH_ATTEMPTS: %s\n' "$publish_attempts" >&2
    printf 'Expected a positive integer.\n' >&2
    exit 1
  fi

  if [[ ! "$publish_initial_wait_seconds" =~ ^[1-9][0-9]*$ ]]; then
    printf 'Invalid MNEMARA_PUBLISH_INITIAL_WAIT_SECONDS: %s\n' "$publish_initial_wait_seconds" >&2
    printf 'Expected a positive integer.\n' >&2
    exit 1
  fi

  if [[ ! "$publish_max_wait_seconds" =~ ^[1-9][0-9]*$ ]]; then
    printf 'Invalid MNEMARA_PUBLISH_MAX_WAIT_SECONDS: %s\n' "$publish_max_wait_seconds" >&2
    printf 'Expected a positive integer.\n' >&2
    exit 1
  fi
}

publish_crate() {
  local crate_name="$1"
  local crate_version
  local output_file
  local attempt=1

  crate_version="$(crate_version "$crate_name")"
  package_crate "$crate_name"

  while (( attempt <= publish_attempts )); do
    output_file="$(mktemp)"
    printf '\n==> cargo publish --manifest-path %s -p %s --allow-dirty (attempt %s/%s)\n' "$manifest_path" "$crate_name" "$attempt" "$publish_attempts"

    if cargo publish --manifest-path "$manifest_path" -p "$crate_name" --allow-dirty >"$output_file" 2>&1; then
      cat "$output_file"
      rm -f "$output_file"
      printf '\n==> published %s %s\n' "$crate_name" "$crate_version"
      return 0
    fi

    if output_indicates_already_published "$output_file"; then
      cat "$output_file"
      rm -f "$output_file"
      printf '\n==> skipping %s %s: this version already exists on crates.io\n' "$crate_name" "$crate_version"
      return 0
    fi

    if output_indicates_retryable_publish_failure "$output_file" && (( attempt < publish_attempts )); then
      cat "$output_file"
      wait_before_publish_retry "$output_file" "$attempt"
      rm -f "$output_file"
      attempt=$((attempt + 1))
      continue
    fi

    cat "$output_file"
    rm -f "$output_file"
    printf '\n==> failed to publish %s %s after %s attempt(s)\n' "$crate_name" "$crate_version" "$attempt" >&2
    return 1
  done
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

publish() {
  local target="${2:-all}"
  local crate_name
  local crates=()
  local target_file

  validate_publish_retry_config

  target_file="$(mktemp)"
  if ! publish_targets "$target" >"$target_file"; then
    rm -f "$target_file"
    exit 1
  fi

  while IFS= read -r crate_name; do
    crates+=("$crate_name")
  done <"$target_file"
  rm -f "$target_file"

  if [[ ${#crates[@]} -eq 0 ]]; then
    printf 'No crates selected for publish target: %s\n' "$target" >&2
    exit 1
  fi

  printf 'Publishing to crates.io in this order:\n'
  for crate_name in "${crates[@]}"; do
    printf '  - %s %s\n' "$crate_name" "$(crate_version "$crate_name")"
  done

  for crate_name in "${crates[@]}"; do
    publish_crate "$crate_name"
  done
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
  $(basename "$0") dry-run-publish all
  $(basename "$0") publish all
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
  publish)
    publish "$@"
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
