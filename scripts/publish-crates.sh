#!/usr/bin/env bash
# Publishes the workspace crates to crates.io in dependency order.
# Safe to re-run: a crate already published at this version is skipped, so
# the second round just continues where the rate limit stopped the first.
#
#   scripts/publish-crates.sh            # all crates, stops on a rate limit
#   scripts/publish-crates.sh --dry-run  # only the first crates verify: a dependent
#                                        # needs its dependencies on the registry
#   MAX_NEW=5 scripts/publish-crates.sh  # stop after 5 uploads (one round)
set -euo pipefail

# Dependency order: a crate's dependencies come before it.
CRATES=(
  obfusku-diagnostics obfusku-core
  obfusku-syntax obfusku-typecheck obfusku-runtime
  obfusku-fmt obfusku-cli obfusku-lsp
)
MAX_NEW="${MAX_NEW:-${#CRATES[@]}}"
DRY=()
[ "${1:-}" = "--dry-run" ] && DRY=(--dry-run)

new=0
for crate in "${CRATES[@]}"; do
  if [ "$new" -ge "$MAX_NEW" ]; then
    echo "round limit ($MAX_NEW) reached before $crate; run again later"
    exit 0
  fi
  echo "==> $crate"
  if out="$(cargo publish --locked -p "$crate" "${DRY[@]}" 2>&1)"; then
    echo "$out" | tail -2
    new=$((new + 1))
  elif grep -q "already exists" <<<"$out"; then
    echo "already published, skipping"
  elif grep -qiE "429|too many requests|rate limit" <<<"$out"; then
    echo "$out" | tail -4
    echo "crates.io rate limit at $crate; run again after the time above" >&2
    exit 75
  else
    echo "$out" >&2
    exit 1
  fi
done
echo "done: $new new, ${#CRATES[@]} total"
