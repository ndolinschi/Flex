#!/usr/bin/env bash
# Phase 3.4 — provider matrix live-smoke (env-gated).
#
# Runs one short `flex run` turn per provider whose API key (or local daemon)
# is present. Missing keys are skipped, never fail the job — safe for nightly
# CI without secrets. Exit 1 only when at least one attempted provider fails.
#
# Usage:
#   ./packages/sdk/scripts/provider-matrix-smoke.sh
#   FLEX_BIN=~/.local/bin/flex ./packages/sdk/scripts/provider-matrix-smoke.sh
#
# Optional:
#   PROVIDER_MATRIX="anthropic openai deepseek"  # subset / override
#   MATRIX_PROMPT="reply with the single word pong"
#
# Follow-ups (not automated here): fallback-chain turns, Win/Linux (3.5),
# large-repo battery (3.3).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
FLEX_BIN="${FLEX_BIN:-flex}"
PROMPT="${MATRIX_PROMPT:-Reply with the single word pong and nothing else.}"
WORKDIR="${MATRIX_WORKDIR:-$ROOT}"

if ! command -v "$FLEX_BIN" >/dev/null 2>&1; then
  if [[ -x "$ROOT/packages/sdk/target/release/flex" ]]; then
    FLEX_BIN="$ROOT/packages/sdk/target/release/flex"
  elif [[ -x "$HOME/.local/bin/flex" ]]; then
    FLEX_BIN="$HOME/.local/bin/flex"
  else
    echo "provider-matrix: flex binary not found (set FLEX_BIN or run ./scripts/install_mac.sh / ./scripts/install_windows.sh)" >&2
    echo "provider-matrix: skipping (no binary) — not a CI failure" >&2
    exit 0
  fi
fi

# Resolve the env var name that gates a provider (empty = always attempt).
key_for() {
  case "$1" in
    anthropic) echo ANTHROPIC_API_KEY ;;
    openai) echo OPENAI_API_KEY ;;
    gemini) echo GEMINI_API_KEY ;;
    deepseek) echo DEEPSEEK_API_KEY ;;
    openrouter) echo OPENROUTER_API_KEY ;;
    groq) echo GROQ_API_KEY ;;
    mistral) echo MISTRAL_API_KEY ;;
    xai) echo XAI_API_KEY ;;
    copilot) echo GITHUB_TOKEN ;;
    ollama) echo "" ;;
    *) echo "" ;;
  esac
}

if [[ -n "${PROVIDER_MATRIX:-}" ]]; then
  # shellcheck disable=SC2206
  MATRIX=( $PROVIDER_MATRIX )
else
  MATRIX=(anthropic openai gemini deepseek ollama openrouter groq mistral xai)
fi

attempted=0
failed=0
skipped=0

echo "provider-matrix: binary=$FLEX_BIN workdir=$WORKDIR"
echo "provider-matrix: prompt=$PROMPT"

for id in "${MATRIX[@]}"; do
  key_var="$(key_for "$id")"
  if [[ -n "$key_var" ]]; then
    # Indirect expansion without nounset blowing up on missing keys.
    eval "key_val=\${$key_var:-}"
    if [[ -z "$key_val" ]]; then
      echo "  skip  $id (missing $key_var)"
      skipped=$((skipped + 1))
      continue
    fi
  fi

  # Ollama has no API key — probe the local daemon before attempting a turn.
  if [[ "$id" == "ollama" ]]; then
    if ! curl -fsS --max-time 1 http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
      echo "  skip  ollama (daemon not reachable at :11434)"
      skipped=$((skipped + 1))
      continue
    fi
  fi

  attempted=$((attempted + 1))
  echo "  run   $id …"
  set +e
  out="$("$FLEX_BIN" run -p "$PROMPT" --provider "$id" --workdir "$WORKDIR" 2>&1)"
  status=$?
  set -e
  if [[ $status -ne 0 ]]; then
    echo "  FAIL  $id (exit $status)"
    echo "$out" | tail -n 40
    failed=$((failed + 1))
  else
    echo "  ok    $id"
  fi
done

echo "provider-matrix: attempted=$attempted failed=$failed skipped=$skipped"
if [[ $attempted -eq 0 ]]; then
  echo "provider-matrix: no providers had credentials — soft-pass"
  exit 0
fi
if [[ $failed -gt 0 ]]; then
  exit 1
fi
exit 0
