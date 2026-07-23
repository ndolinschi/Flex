
export const isCaretAtEndOfLine = (draft: string, caret: number): boolean => {
  if (caret < 0 || caret > draft.length) return false
  if (caret === draft.length) return true
  return draft[caret] === "\n"
}

export const acceptInlineSuggestion = (
  draft: string,
  caret: number,
  suggestion: string,
): { draft: string; caret: number } => {
  if (!suggestion) return { draft, caret }
  const next = draft.slice(0, caret) + suggestion + draft.slice(caret)
  return { draft: next, caret: caret + suggestion.length }
}

export const INLINE_COMPLETION_MIN_PREFIX = 4

export const INLINE_COMPLETION_DEBOUNCE_MS = 300

/** Cap IPC payload: last N chars of prefix + first M of suffix. */
export const INLINE_COMPLETION_MAX_PREFIX = 8_000
export const INLINE_COMPLETION_MAX_SUFFIX = 2_000

export const capCompletionContext = (
  prefix: string,
  suffix: string,
): { prefix: string; suffix: string } => ({
  prefix:
    prefix.length > INLINE_COMPLETION_MAX_PREFIX
      ? prefix.slice(-INLINE_COMPLETION_MAX_PREFIX)
      : prefix,
  suffix:
    suffix.length > INLINE_COMPLETION_MAX_SUFFIX
      ? suffix.slice(0, INLINE_COMPLETION_MAX_SUFFIX)
      : suffix,
})

export const RECOMMENDED_OLLAMA_MODEL = "qwen2.5:0.5b"

export const OLLAMA_PULL_COMMAND = `ollama pull ${RECOMMENDED_OLLAMA_MODEL}`

export const normalizeCompletionModelId = (
  providerId: string,
  modelId: string,
): string => {
  const prefix = `${providerId}/`
  return modelId.startsWith(prefix) ? modelId.slice(prefix.length) : modelId
}

export const qualifiedCompletionModelId = (
  providerId: string,
  modelId: string,
): string =>
  modelId.includes("/") ? modelId : `${providerId}/${modelId}`
