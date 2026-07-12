import type { QueryClient } from "@tanstack/react-query"
import type { SessionMeta } from "../types"
import { isDefaultSessionTitle } from "../types"
import { suggestSessionTitle, updateSession } from "../tauri"

export const SESSIONS_QUERY_KEY = ["sessions"] as const

/** Sessions that have already had (or are currently having) an auto-title
 * attempted — fire-once gate so later turns never re-title a session, and a
 * slow in-flight suggestion can't be kicked off twice. Mirrors the
 * fire-once pattern in `agentTerminal.ts`'s `autoActivatedCallIds`. */
const autoTitledSessionIds = new Set<string>()

/** The raw-prompt-derived title Composer's `handleSend` set synchronously
 * for a session (via `titleFromPrompt(text)`), keyed by session id —
 * recorded by `markRawPromptTitle` right after that `updateSession` call
 * succeeds. Lets `maybeAutoTitleSession` recognize "title still equals the
 * raw first-prompt text" (requirement: never overwrite a title the user
 * set manually) without re-deriving it from timeline state, which this
 * module has no access to. Cleared once consumed so a later manual rename
 * to the exact same string can't accidentally look auto-generated. */
const rawPromptTitleBySession = new Map<string, string>()

/** Called by Composer right after it sets a session's placeholder title to
 * `titleFromPrompt(text)` on the first send. */
export const markRawPromptTitle = (sessionId: string, title: string): void => {
  rawPromptTitleBySession.set(sessionId, title)
}

/** Cursor-style semantic auto-title: after a session's FIRST turn
 * completes, replace the placeholder/raw-prompt title with a short
 * LLM-generated summary (2-5 words) of the task. Runs fire-and-forget —
 * never blocks or delays the turn, and any failure (no model, provider
 * error, offline) just leaves the existing title in place.
 *
 * Only fires when the session's current title is still exactly what
 * Composer set synchronously at send time: either `DEFAULT_SESSION_TITLE`
 * (session had no prior title at all — e.g. this command ran before
 * Composer's own rename-from-prompt effect) or the raw first-prompt title
 * recorded via `markRawPromptTitle`. A title the user renamed manually in
 * between is never touched — it won't match either check.
 */
export const maybeAutoTitleSession = (
  meta: SessionMeta | undefined,
  queryClient: QueryClient | undefined,
): void => {
  if (!meta) return
  const title = meta.title?.trim() ?? ""
  const rawPromptTitle = rawPromptTitleBySession.get(meta.id)
  const eligible = isDefaultSessionTitle(meta.title) || title === rawPromptTitle
  if (!eligible) return
  if (autoTitledSessionIds.has(meta.id)) return
  autoTitledSessionIds.add(meta.id)
  rawPromptTitleBySession.delete(meta.id)

  const promptText = title
  if (!promptText) return

  void suggestSessionTitle(meta.id, promptText)
    .then((title) => {
      const trimmed = title.trim()
      if (!trimmed) return
      return updateSession(meta.id, { title: trimmed }).then(() => {
        void queryClient?.invalidateQueries({ queryKey: SESSIONS_QUERY_KEY })
      })
    })
    .catch(() => {
      // Non-fatal — the session keeps its placeholder/raw-prompt title.
    })
}

/** Test-only reset for the fire-once gate. */
export const __resetAutoTitleGateForTests = () => {
  autoTitledSessionIds.clear()
}
