#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# The old name must be gone from every source file (this test excluded).
if grep -rn "calc_sum" lib main.sh report.sh; then
  echo "FAIL: calc_sum still present"
  exit 1
fi

# The new name must exist and behavior must be intact.
grep -qn "compute_total" lib/totals.sh || { echo "FAIL: compute_total missing"; exit 1; }
[ "$(bash main.sh)" = "total: 6" ] || { echo "FAIL: main.sh output changed"; exit 1; }
[ "$(bash report.sh 5 5)" = "report: items=2 value=10" ] || { echo "FAIL: report.sh output changed"; exit 1; }
echo "OK"
