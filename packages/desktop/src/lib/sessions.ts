import type { CreateSessionInput, IsolationPolicy, SessionMeta } from "./types"
import { DEFAULT_SESSION_TITLE, isDefaultSessionTitle } from "./types"

/** True when an engine error means the session id no longer exists (engine's
 * `StoreError::SessionNotFound` → "session {id} not found", surfaced via
 * `toInvokeError`). Distinct from other resume/delete failures — retrying a
 * not-found is meaningless, so callers should self-heal instead. */
export const isSessionNotFoundError = (message: string): boolean =>
  /session\s+\S+\s+not found/i.test(message) || /session not found/i.test(message)

/** Find an unused "New Agent" session for this project (design: one draft per cwd).
 * Matches on `base_cwd ?? cwd` so isolated drafts (cwd = worktree) still group
 * with their project. Only returns an *unprovisioned* draft — leftover
 * create-time worktrees from before deferred isolation must not be reused as
 * the empty New Agent surface (they still carry WorkspaceProvisioned history). */
export const findDraftSession = (
  sessions: SessionMeta[],
  cwd?: string | null,
): SessionMeta | undefined => {
  const key = cwd?.trim() || ""
  return sessions.find((s) => {
    if (!isDefaultSessionTitle(s.title)) return false
    // Already provisioned (create-time leftover or first-prompt worktree).
    if (s.base_cwd || s.workspace_id) return false
    const sessionCwd = s.cwd?.trim() || ""
    return sessionCwd === key
  })
}

/** Resolve cwd preference: explicit → recent → active session. */
export const resolveCreateCwd = (
  sessions: SessionMeta[],
  activeSessionId: string | null,
  recentCwds: string[],
  explicitCwd?: string,
): string | undefined => {
  if (explicitCwd?.trim()) return explicitCwd.trim()
  if (recentCwds[0]?.trim()) return recentCwds[0].trim()
  const active = sessions.find((s) => s.id === activeSessionId)
  return active?.cwd?.trim() || undefined
}

export const newAgentCreateInput = (
  cwd?: string,
  model?: string | null,
  isolation?: IsolationPolicy | null,
  reuseWorkspaceId?: string | null,
): CreateSessionInput => ({
  title: DEFAULT_SESSION_TITLE,
  ...(cwd ? { cwd } : {}),
  ...(model ? { model } : {}),
  // Omitted when unset — `create_session` then falls back to the provider
  // profile's `default_isolation` (see commands.rs::create_session).
  ...(isolation ? { isolation } : {}),
  // Only forwarded when the user picked an existing worktree; the backend
  // ignores the hint unless the resolved isolation policy wants one.
  ...(reuseWorkspaceId ? { reuseWorkspaceId } : {}),
})
