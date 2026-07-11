import { useAppStore, sessionScopeKey } from "../../stores/appStore"
import { browserNavigate, browserOpen } from "../tauri"

/** Matches local dev-server URLs in exec output (localhost / 127.0.0.1 / 0.0.0.0,
 * optional port, optional path). Used by the "Open in Browser" toast below. */
const DEV_SERVER_URL_RE =
  /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0)(?::\d+)?(?:\/\S*)?/g

/** Normalize a detected dev-server URL for display/navigation — `0.0.0.0`
 * is not directly reachable from the app's webview, so route it through
 * `localhost` (same port/path). */
const normalizeDevServerUrl = (raw: string): string => {
  const trimmed = raw.replace(/[)\]}>,.;!?'"]+$/, "")
  return trimmed.replace(/^(https?:\/\/)0\.0\.0\.0/, "$1localhost")
}

/** Dev-server URLs already toasted this session run, keyed by
 * `${sessionId}${origin}` (origin = protocol+host+port, no path) —
 * a URL only triggers once per session run; a different port triggers again. */
const toastedDevServerOrigins = new Set<string>()

/** Opens the given URL in the app's built-in browser (right panel Browser
 * tab), taking ownership of the shared webview/iframe for `sessionId` —
 * mirrors BrowserTab's own omnibar-driven `commitNavigate` flow. */
const openDevServerUrlInBrowser = (sessionId: string, url: string) => {
  const store = useAppStore.getState()
  const sessionKey = sessionScopeKey(sessionId)
  const wasStarted = !!store.browserBySession[sessionKey]?.started
  store.setBrowserSessionState(sessionKey, { loading: true, url })
  store.setBrowserOwnerSessionId(sessionKey)
  store.setRightPanelOpen(true)
  store.setRightPanelTab("browser")
  if (wasStarted) {
    void browserNavigate(url).catch(() => {})
  } else {
    void browserOpen(url).catch(() => {})
  }
}

/** Scan exec output for a local dev-server URL and, the first time one shows
 * up for a given session+origin, offer an "Open in Browser" toast. v1 only
 * surfaces this for the currently-active session — a background session's
 * dev server won't toast (no owner-takeover of a session the user isn't
 * looking at). */
export const maybeToastDevServerUrl = (sessionId: string, text: string) => {
  const store = useAppStore.getState()
  if (sessionId !== store.activeSessionId) return

  const matches = text.match(DEV_SERVER_URL_RE)
  if (!matches) return

  for (const raw of matches) {
    const url = normalizeDevServerUrl(raw)
    let origin: string
    try {
      origin = new URL(url).origin
    } catch {
      continue
    }
    const dedupKey = `${sessionId}${origin}`
    if (toastedDevServerOrigins.has(dedupKey)) continue
    toastedDevServerOrigins.add(dedupKey)

    store.pushToast(`Dev server detected at ${url}`, "success", {
      label: "Open in Browser",
      onAction: () => openDevServerUrlInBrowser(sessionId, url),
    })
  }
}
