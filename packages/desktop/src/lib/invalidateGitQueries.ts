import type { QueryClient } from "@tanstack/react-query"

export type GitInvalidateScope = {
  cwd?: string
  sessionId?: string
}

const GIT_ROOT_KEYS = new Set([
  "git-status",
  "git-is-repo",
  "git-has-remote",
  "git-pr-status",
  "git-pr-diff",
  "git-pr-draft",
])

const hasScope = (scope?: GitInvalidateScope): scope is GitInvalidateScope =>
  !!scope && (!!scope.cwd || !!scope.sessionId)

/**
 * Match a react-query cache entry against an optional cwd/session scope.
 *
 * Keys observed in the desktop app:
 * - ["git-status", cwd, sessionId]
 * - ["git-is-repo", cwd?]
 * - ["git-has-remote", cwd]
 * - ["git-pr-status", cwd]
 * - ["git-pr-diff", cwd, prNumber]
 * - ["git-pr-draft", cwd]
 */
export const matchesGitScope = (
  queryKey: readonly unknown[],
  scope: GitInvalidateScope,
): boolean => {
  if (!Array.isArray(queryKey) || typeof queryKey[0] !== "string") return false
  if (!GIT_ROOT_KEYS.has(queryKey[0])) return false

  if (queryKey[0] === "git-status") {
    const qCwd = queryKey[1]
    const qSessionId = queryKey[2]
    if (scope.sessionId != null && qSessionId === scope.sessionId) return true
    if (scope.cwd != null && qCwd === scope.cwd) return true
    return false
  }

  // Cwd-scoped keys (is-repo / remote / pr*). Session-only scope cannot match.
  if (scope.cwd != null) {
    return queryKey[1] === scope.cwd
  }
  return false
}

/** Invalidate git-related queries. Without scope: global (backward compatible). */
export const invalidateGitQueries = (
  queryClient: QueryClient,
  scope?: GitInvalidateScope,
): void => {
  if (!hasScope(scope)) {
    void queryClient.invalidateQueries({ queryKey: ["git-status"] })
    void queryClient.invalidateQueries({ queryKey: ["git-is-repo"] })
    void queryClient.invalidateQueries({ queryKey: ["git-has-remote"] })
    void queryClient.invalidateQueries({ queryKey: ["git-pr-status"] })
    void queryClient.invalidateQueries({ queryKey: ["git-pr-diff"] })
    void queryClient.invalidateQueries({ queryKey: ["git-pr-draft"] })
    return
  }

  void queryClient.invalidateQueries({
    predicate: (q) => matchesGitScope(q.queryKey, scope),
  })
}

const debounceTimers = new Map<string, ReturnType<typeof setTimeout>>()

const scopeFingerprint = (scope?: GitInvalidateScope): string =>
  `${scope?.cwd ?? ""}\0${scope?.sessionId ?? ""}`

/**
 * Debounced git invalidation — useful for FS-mutating tool bursts during a turn.
 * Default delay 400ms; trailing-edge only (last call wins per scope key).
 */
export const invalidateGitQueriesDebounced = (
  queryClient: QueryClient,
  scope?: GitInvalidateScope,
  delayMs = 400,
): void => {
  const key = scopeFingerprint(scope)
  const existing = debounceTimers.get(key)
  if (existing !== undefined) {
    clearTimeout(existing)
  }
  debounceTimers.set(
    key,
    setTimeout(() => {
      debounceTimers.delete(key)
      invalidateGitQueries(queryClient, scope)
    }, delayMs),
  )
}
