#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
. src/math.sh

[ "$(add 2 3)" = "5" ] || { echo "FAIL: add 2 3 = $(add 2 3), want 5"; exit 1; }
[ "$(add 10 0)" = "10" ] || { echo "FAIL: add 10 0 = $(add 10 0), want 10"; exit 1; }
[ "$(double 4)" = "8" ] || { echo "FAIL: double 4 = $(double 4), want 8"; exit 1; }
echo "OK"
