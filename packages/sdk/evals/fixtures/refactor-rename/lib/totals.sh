#!/usr/bin/env bash
# Accumulation helpers.

calc_sum() {
  local total=0
  for n in "$@"; do
    total=$(( total + n ))
  done
  echo "$total"
}
