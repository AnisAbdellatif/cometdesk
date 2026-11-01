#!/usr/bin/env bash
# Fails if a project .rs file exceeds MAX lines (default 600) or lacks a leading `//!` header.
# Keeps files small enough for an LLM (or a person) to read whole. The size check skips
# vendor/, target/, @generated files and tests; the header check skips only the first three.
set -euo pipefail
cd "$(dirname "$0")/.."
MAX="${MAX:-600}"
fail=0
while IFS= read -r f; do
  case "$f" in vendor/*|*/target/*|target/*) continue;; esac
  head -3 "$f" | grep -q '@generated' && continue
  head -1 "$f" | grep -q '^//!' || { echo "missing //! header: $f"; fail=1; }
  case "$f" in */tests.rs|*/tests/*) continue;; esac
  n=$(wc -l < "$f")
  if [ "$n" -gt "$MAX" ]; then echo "too long ($n > $MAX): $f"; fail=1; fi
done < <(git ls-files '*.rs')
exit $fail
