#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
. lib/totals.sh

echo "total: $(calc_sum 1 2 3)"
