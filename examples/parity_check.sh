#!/usr/bin/env bash
# Full byte-parity check: Rust vs Python pipeline on the shared samples.
# Prints a per-case PASS/DIFF verdict. No deletion needed — uses a fresh dir
# stamped with $$ so repeated runs never collide.
set -euo pipefail

CRATE=/home/danarchy/workspace/logscope-rs
PYDIR=/home/danarchy/workspace/logscope
OUT="$(mktemp -d /tmp/logscope_diff.XXXXXX)"
export LOGSCOPE_DIFF_DIR="$OUT"

echo "diff dir: $OUT"

# Rust side (reads LOGSCOPE_DIFF_DIR)
(cd "$CRATE" && LOGSCOPE_DIFF_DIR="$OUT" cargo run --example full_diff 2>/dev/null)

# Python side (reads LOGSCOPE_DIFF_DIR)
(cd "$PYDIR" && source .venv/bin/activate && LOGSCOPE_DIFF_DIR="$OUT" python "$CRATE/examples/full_diff.py")

status=0
for pair in "simple.out simple.out.py" "folder.out folder.out.py" "zip.out zip.out.py" "tar.out tar.out.py"; do
  set -- $pair
  if diff -u "$OUT/$1" "$OUT/$2" >/dev/null 2>&1; then
    echo "PASS: $1"
  else
    echo "DIFF: $1 vs $2"
    diff -u "$OUT/$1" "$OUT/$2" || true
    status=1
  fi
done
exit $status
