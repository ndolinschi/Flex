import { useEffect, useRef, type RefObject } from "react"
import { Terminal, type ITheme } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import "@xterm/xterm/css/xterm.css"
import { terminalResize, terminalWrite } from "../lib/tauri"
import { subscribeTerminal } from "../lib/terminalBus"

/**
 * xterm's canvas theme parser does not accept CSS Color Level 4
 * `rgb(20 20 20 / 0.92)` (space-separated + slash alpha) from our tokens.
 * Convert to `#rrggbb` / `rgba(r,g,b,a)` so text stays visible.
 */
const cssColorToXterm = (raw: string, fallback: string): string => {
  const value = raw.trim()
  if (!value) return fallback
  if (value.startsWith("#")) return value

  const modern = value.match(
    /^rgba?\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+%?))?\s*\)$/i,
  )
  if (modern) {
    const r = Math.round(Number(modern[1]))
    const g = Math.round(Number(modern[2]))
    const b = Math.round(Number(modern[3]))
    const aRaw = modern[4]
    if (aRaw == null) {
      return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`
    }
    const a = aRaw.endsWith("%")
      ? Number(aRaw.slice(0, -1)) / 100
      : Number(aRaw)
    return `rgba(${r}, ${g}, ${b}, ${Number.isFinite(a) ? a : 1})`
  }

  // Legacy rgb(r, g, b) / rgba(r, g, b, a) — already xterm-safe.
  if (/^rgba?\(/i.test(value)) return value
  return fallback
}

/** Read live CSS custom properties so the terminal tracks theme switches. */
const readThemeVars = (): ITheme => {
  const styles = getComputedStyle(document.documentElement)
  const read = (name: string, fallback: string) =>
    cssColorToXterm(styles.getPropertyValue(name), fallback)

  return {
    background: read("--color-chrome", "#0c0e11"),
    foreground: read("--color-ink", "#eef2fa"),
    cursor: read("--color-ink", "#eef2fa"),
    cursorAccent: read("--color-chrome", "#0c0e11"),
    selectionBackground: read("--color-fill-2", "rgba(255, 255, 255, 0.07)"),
  }
}

export type UseTerminalOptions = {
  /**
   * Read-only terminal (e.g. the agent's terminal, mirrored from
   * `exec_chunk` session-events). Disables stdin and skips the backend
   * `terminalWrite`/`terminalResize` IPC calls — there is no PTY behind
   * this id, only local fit + bus playback.
   */
  readOnly?: boolean
}

/**
 * Owns one xterm.js `Terminal` instance bound to a backend PTY session `id`.
 * Mount one `useTerminal` per rendered terminal container; the hook wires
 * input/output/resize/exit and disposes everything on unmount.
 */
export const useTerminal = (
  id: string,
  containerRef: RefObject<HTMLDivElement | null>,
  active = true,
  options?: UseTerminalOptions,
) => {
  const readOnly = options?.readOnly ?? false
  const fitRef = useRef<FitAddon | null>(null)
  const termRef = useRef<Terminal | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const theme = readThemeVars()
    const term = new Terminal({
      fontFamily: 'Menlo, Monaco, ui-monospace, "SF Mono", monospace',
      fontSize: 12,
      lineHeight: 1.4,
      cursorBlink: true,
      scrollback: 5000,
      theme,
      allowProposedApi: false,
      disableStdin: readOnly,
    })
    termRef.current = term

    const fitAddon = new FitAddon()
    fitRef.current = fitAddon
    term.loadAddon(fitAddon)

    term.open(container)
    // #region agent log
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H9",
        location: "useTerminal.ts:open",
        message: "xterm opened with normalized theme",
        data: {
          id,
          width: container.clientWidth,
          height: container.clientHeight,
          foreground: theme.foreground,
          background: theme.background,
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
    // #endregion
    if (container.clientWidth > 0 && container.clientHeight > 0) {
      fitAddon.fit()
      if (!readOnly) void terminalResize(id, term.cols, term.rows)
    }

    const dataDisposable = readOnly
      ? null
      : term.onData((data) => {
          void terminalWrite(id, data)
        })

    const themeObserver = new MutationObserver(() => {
      term.options.theme = readThemeVars()
    })
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    })

    const resizeObserver = new ResizeObserver(() => {
      if (container.clientWidth === 0 || container.clientHeight === 0) return
      fitAddon.fit()
      if (!readOnly) void terminalResize(id, term.cols, term.rows)
    })
    resizeObserver.observe(container)

    // Synchronous subscribe via the bus: replays buffered scrollback first,
    // so output emitted before this instance mounted (shell prompt, StrictMode
    // remount gaps) is never lost.
    const unsubscribe = subscribeTerminal(id, (data) => term.write(data))

    if (active) {
      term.focus()
    }

    return () => {
      themeObserver.disconnect()
      resizeObserver.disconnect()
      dataDisposable?.dispose()
      unsubscribe()
      fitRef.current = null
      termRef.current = null
      term.dispose()
    }
  }, [id, containerRef, readOnly])

  useEffect(() => {
    if (!active) return
    termRef.current?.focus()
    const container = containerRef.current
    if (!container || container.clientWidth === 0) return
    fitRef.current?.fit()
  }, [active, containerRef])

  const fit = () => {
    const container = containerRef.current
    if (!container) return
    if (container.clientWidth === 0 || container.clientHeight === 0) return
    fitRef.current?.fit()
  }

  return { fit }
}
