
const scheduleIdle = (fn: () => void, timeoutMs = 2_500): (() => void) => {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(() => fn(), { timeout: timeoutMs })
    return () => cancelIdleCallback(id)
  }
  const t = window.setTimeout(fn, Math.min(timeoutMs, 800))
  return () => window.clearTimeout(t)
}

let started = false

export const startDesktopIdlePrefetch = (): void => {
  if (started || typeof window === "undefined") return
  started = true

  scheduleIdle(() => {
    void import("../components/organisms/right-panel/FilesTab")
    void import("../components/organisms/terminal/TerminalTab")
    void import("../components/organisms/BrowserTab")
    void import("../components/organisms/right-panel/StatusTab")
    void import("../lib/markdownHighlight")
    void import("../plugins/database/DatabaseTab")
    void import("../plugins/components/ComponentsTab")
    void import("../plugins/artifacts/ArtifactsTab")
  })
}
