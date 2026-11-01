#!/usr/bin/env bash
# Fails if a project .rs file exceeds MAX lines (default 600). Keeps files small enough for an
# LLM (or a person) to read whole. Skips vendor/, target/, @generated files and tests.rs.
set -euo pipefail
cd "$(dirname "$0")/.."
MAX="${MAX:-600}"
fail=0
while IFS= read -r f; do
  case "$f" in vendor/*|*/target/*|target/*|*/tests.rs|*/tests/*) continue;; esac
  head -3 "$f" | grep -q '@generated' && continue
  n=$(wc -l < "$f")
  if [ "$n" -gt "$MAX" ]; then echo "too long ($n > $MAX): $f"; fail=1; fi
done < <(git ls-files '*.rs')
exit $fail
