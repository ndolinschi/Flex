import type { SessionEvent } from "../types"
import { useAppStore, sessionScopeKey } from "../../stores/appStore"
import { browserOpen } from "../tauri"
import { pathFromInput } from "../toolPresentation"

const BROWSER_TOOLS = new Set([
  "browsernavigate",
  "browserscreenshot",
  "browsereval",
  "browserclick",
  "browserconsole",
  "browseropendevtools",
])

const revealedForCall = new Set<string>()

const urlFromInput = (input: unknown): string | null => {
  if (!input || typeof input !== "object") return null
  const rec = input as Record<string, unknown>
  for (const key of ["url", "href", "uri"]) {
    const v = rec[key]
    if (typeof v === "string" && v.trim()) return v.trim()
  }
  return pathFromInput(input)
}

/** Reveal the embedded Browser panel when Browser* tools run (mirrors
 * Artifacts / Terminal auto-open). Creates the native webview via
 * `browser_open` when BrowserNavigate supplies a URL. */
export const maybeRevealBrowser = (
  event: SessionEvent,
  opts?: { activeSessionId?: string | null },
): void => {
  const { payload } = event
  if (payload.kind !== "tool_call_updated") return

  const { call } = payload
  const toolName = call.tool_name.toLowerCase()
  if (!BROWSER_TOOLS.has(toolName)) return

  const state = call.status.state
  if (
    state !== "pending" &&
    state !== "running" &&
    state !== "awaiting_permission"
  ) {
    return
  }

  const store = useAppStore.getState()
  const activeId = opts?.activeSessionId ?? store.activeSessionId
  if (event.session_id !== activeId) return

  const callKey = `${event.session_id}:${call.id}`
  if (revealedForCall.has(callKey)) return
  revealedForCall.add(callKey)
  if (revealedForCall.size > 200) {
    const oldest = revealedForCall.values().next().value
    if (oldest) revealedForCall.delete(oldest)
  }

  store.openToolBesideChat(event.session_id, "browser")
  store.setBrowserOwnerSessionId(sessionScopeKey(event.session_id))

  if (toolName === "browsernavigate") {
    const url = urlFromInput(call.input)
    if (url) {
      store.setBrowserSessionState(sessionScopeKey(event.session_id), {
        url,
        loading: true,
        started: true,
        loadError: null,
      })
      void browserOpen(url).catch(() => {})
    }
  }
}
