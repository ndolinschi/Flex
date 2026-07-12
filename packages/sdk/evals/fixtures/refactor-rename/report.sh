#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
. lib/totals.sh

echo "report: items=$# value=$(calc_sum "$@")"
