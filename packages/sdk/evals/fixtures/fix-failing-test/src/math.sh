#!/usr/bin/env bash
# Tiny math helpers used by the test suite.

add() {
  # BUG: subtracts instead of adding.
  echo $(( $1 - $2 ))
}

double() {
  echo $(( $1 * 2 ))
}
