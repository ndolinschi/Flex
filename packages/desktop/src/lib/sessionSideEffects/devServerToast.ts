import { useAppStore, sessionScopeKey } from "../../stores/appStore"
import { browserNavigate, browserOpen } from "../tauri"

const DEV_SERVER_URL_RE =
  /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0)(?::\d+)?(?:\/\S*)?/g

const normalizeDevServerUrl = (raw: string): string => {
  const trimmed = raw.replace(/[)\]}>,.;!?'"]+$/, "")
  return trimmed.replace(/^(https?:\/\/)0\.0\.0\.0/, "$1localhost")
}

const toastedDevServerOrigins = new Set<string>()

const openDevServerUrlInBrowser = (sessionId: string, url: string) => {
  const store = useAppStore.getState()
  const sessionKey = sessionScopeKey(sessionId)
  const wasStarted = !!store.browserBySession[sessionKey]?.started
  store.setBrowserSessionState(sessionKey, { loading: true, url })
  store.setBrowserOwnerSessionId(sessionKey)
  store.openToolBesideChat(sessionId, "browser")
  if (wasStarted) {
    void browserNavigate(url).catch(() => {})
  } else {
    void browserOpen(url).catch(() => {})
  }
}

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
