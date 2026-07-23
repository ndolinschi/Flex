import type { QueryClient } from "@tanstack/react-query"
import type { SessionMeta } from "../types"
import { isDefaultSessionTitle } from "../types"
import { suggestSessionTitle, updateSession } from "../tauri"

export const SESSIONS_QUERY_KEY = ["sessions"] as const

const autoTitledSessionIds = new Set<string>()

const rawPromptTitleBySession = new Map<string, string>()

export const markRawPromptTitle = (sessionId: string, title: string): void => {
  rawPromptTitleBySession.set(sessionId, title)
}

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
    })
}

export const __resetAutoTitleGateForTests = () => {
  autoTitledSessionIds.clear()
}
