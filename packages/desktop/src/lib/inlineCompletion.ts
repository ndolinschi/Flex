/** Pure helpers for ghost-text prompt completion (composer + Prompt tab). */

/** True when the caret sits at end-of-line or end-of-draft (v1 ghost target). */
export const isCaretAtEndOfLine = (draft: string, caret: number): boolean => {
  if (caret < 0 || caret > draft.length) return false
  if (caret === draft.length) return true
  return draft[caret] === "\n"
}

/** Insert `suggestion` at `caret`, returning the next draft + caret. */
export const acceptInlineSuggestion = (
  draft: string,
  caret: number,
  suggestion: string,
): { draft: string; caret: number } => {
  if (!suggestion) return { draft, caret }
  const next = draft.slice(0, caret) + suggestion + draft.slice(caret)
  return { draft: next, caret: caret + suggestion.length }
}

/** Minimum prefix length before we ask the model. */
export const INLINE_COMPLETION_MIN_PREFIX = 8

/** Debounce before invoking `complete_prompt_inline`. */
export const INLINE_COMPLETION_DEBOUNCE_MS = 300

/** Recommended small Ollama model for the setup modal. */
export const RECOMMENDED_OLLAMA_MODEL = "qwen2.5:0.5b"

export const OLLAMA_PULL_COMMAND = `ollama pull ${RECOMMENDED_OLLAMA_MODEL}`
