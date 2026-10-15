#!/usr/bin/env bash
# Validate every commit subject in a range (CI side of the commit-msg hook).
# Usage: check-commit-range.sh <base-ref> <head-ref>      e.g. origin/main HEAD
set -euo pipefail

base="${1:?usage: $0 <base-ref> <head-ref>}"
head="${2:?usage: $0 <base-ref> <head-ref>}"
here="$(cd "$(dirname "$0")" && pwd)"
tmp=$(mktemp); trap 'rm -f "$tmp"' EXIT

status=0
while IFS= read -r subject; do
  printf '%s\n' "$subject" > "$tmp"
  "$here/check-commit-message.sh" "$tmp" || status=1
done < <(git log --format=%s "$base..$head")
exit $status
