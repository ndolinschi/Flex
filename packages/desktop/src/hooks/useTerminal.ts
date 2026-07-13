import { useEffect, useRef, type RefObject } from "react"
import type { ITheme, Terminal } from "@xterm/xterm"
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
    background: read("--color-chrome", "#141414"),
    foreground: read("--color-ink", "#f2f2f2"),
    cursor: read("--color-ink", "#f2f2f2"),
    cursorAccent: read("--color-chrome", "#141414"),
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

type FitAddonLike = { fit: () => void }

/**
 * Owns one xterm.js `Terminal` instance bound to a backend PTY session `id`.
 * Mount one `useTerminal` per rendered terminal container; the hook wires
 * input/output/resize/exit and disposes everything on unmount.
 *
 * xterm + CSS are dynamic-imported inside the effect so the chat shell does
 * not pay for the terminal vendor chunk until a terminal tab mounts.
 */
export const useTerminal = (
  id: string,
  containerRef: RefObject<HTMLDivElement | null>,
  active = true,
  options?: UseTerminalOptions,
) => {
  const readOnly = options?.readOnly ?? false
  const fitRef = useRef<FitAddonLike | null>(null)
  const termRef = useRef<Terminal | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    let cancelled = false
    let term: Terminal | null = null
    let dataDisposable: { dispose: () => void } | null = null
    let themeObserver: MutationObserver | null = null
    let resizeObserver: ResizeObserver | null = null
    let unsubscribe: (() => void) | null = null

    void (async () => {
      const [{ Terminal: TerminalCtor }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ])
      await import("@xterm/xterm/css/xterm.css")
      if (cancelled) return

      const theme = readThemeVars()
      term = new TerminalCtor({
        fontFamily: 'Menlo, Monaco, ui-monospace, "SF Mono", monospace',
        fontSize: 12,
        lineHeight: 1.4,
        cursorBlink: true,
        scrollback: 5000,
        theme,
        allowProposedApi: false,
        disableStdin: readOnly,
      })
      if (cancelled) {
        term.dispose()
        term = null
        return
      }
      termRef.current = term

      const fitAddon = new FitAddon()
      fitRef.current = fitAddon
      term.loadAddon(fitAddon)

      term.open(container)
      if (container.clientWidth > 0 && container.clientHeight > 0) {
        fitAddon.fit()
        if (!readOnly) void terminalResize(id, term.cols, term.rows)
      }

      dataDisposable = readOnly
        ? null
        : term.onData((data) => {
            void terminalWrite(id, data)
          })

      themeObserver = new MutationObserver(() => {
        if (!term) return
        term.options.theme = readThemeVars()
      })
      themeObserver.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["data-theme"],
      })

      resizeObserver = new ResizeObserver(() => {
        if (container.clientWidth === 0 || container.clientHeight === 0) return
        fitAddon.fit()
        if (!readOnly && term) void terminalResize(id, term.cols, term.rows)
      })
      resizeObserver.observe(container)

      // Synchronous subscribe via the bus: replays buffered scrollback first,
      // so output emitted before this instance mounted (shell prompt, StrictMode
      // remount gaps) is never lost.
      unsubscribe = subscribeTerminal(id, (data) => term?.write(data))

      if (cancelled) {
        themeObserver.disconnect()
        resizeObserver.disconnect()
        dataDisposable?.dispose()
        unsubscribe()
        fitRef.current = null
        termRef.current = null
        term.dispose()
        term = null
        return
      }

      if (active) {
        term.focus()
      }
    })()

    return () => {
      cancelled = true
      themeObserver?.disconnect()
      resizeObserver?.disconnect()
      dataDisposable?.dispose()
      unsubscribe?.()
      fitRef.current = null
      termRef.current = null
      term?.dispose()
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
