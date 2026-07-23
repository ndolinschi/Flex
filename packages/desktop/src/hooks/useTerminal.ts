import { useEffect, useRef, type RefObject } from "react"
import type { ITheme, Terminal } from "@xterm/xterm"
import { terminalResize, terminalWrite } from "../lib/tauri"
import { subscribeTerminal } from "../lib/terminalBus"

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

  if (/^rgba?\(/i.test(value)) return value
  return fallback
}

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
  readOnly?: boolean
}

type FitAddonLike = { fit: () => void }

export const useTerminal = (
  id: string,
  containerRef: RefObject<HTMLDivElement | null>,
  active = true,
  options?: UseTerminalOptions,
) => {
  const readOnly = options?.readOnly ?? false
  const fitRef = useRef<FitAddonLike | null>(null)
  const termRef = useRef<Terminal | null>(null)
  const busUnsubRef = useRef<(() => void) | null>(null)
  const activeRef = useRef(active)
  activeRef.current = active

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    let cancelled = false
    let term: Terminal | null = null
    let dataDisposable: { dispose: () => void } | null = null
    let themeObserver: MutationObserver | null = null
    let resizeObserver: ResizeObserver | null = null

    const attachBus = (t: Terminal) => {
      busUnsubRef.current?.()
      busUnsubRef.current = null
      if (!activeRef.current) return
      t.reset()
      busUnsubRef.current = subscribeTerminal(id, (data) => t.write(data))
    }

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
        if (!activeRef.current) return
        if (container.clientWidth === 0 || container.clientHeight === 0) return
        fitAddon.fit()
        if (!readOnly && term) void terminalResize(id, term.cols, term.rows)
      })
      resizeObserver.observe(container)

      attachBus(term)
      if (activeRef.current) term.focus()

      if (cancelled) {
        themeObserver.disconnect()
        resizeObserver.disconnect()
        dataDisposable?.dispose()
        busUnsubRef.current?.()
        busUnsubRef.current = null
        fitRef.current = null
        termRef.current = null
        term.dispose()
        term = null
      }
    })()

    return () => {
      cancelled = true
      themeObserver?.disconnect()
      resizeObserver?.disconnect()
      dataDisposable?.dispose()
      busUnsubRef.current?.()
      busUnsubRef.current = null
      fitRef.current = null
      termRef.current = null
      term?.dispose()
    }
  }, [id, containerRef, readOnly])

  useEffect(() => {
    const term = termRef.current
    if (!term) return
    if (!active) {
      busUnsubRef.current?.()
      busUnsubRef.current = null
      return
    }
    busUnsubRef.current?.()
    term.reset()
    busUnsubRef.current = subscribeTerminal(id, (data) => term.write(data))
    term.focus()
    const container = containerRef.current
    if (container && container.clientWidth > 0) {
      fitRef.current?.fit()
      if (!readOnly) void terminalResize(id, term.cols, term.rows)
    }
  }, [active, id, containerRef, readOnly])

  const fit = () => {
    const container = containerRef.current
    if (!container) return
    if (container.clientWidth === 0 || container.clientHeight === 0) return
    fitRef.current?.fit()
  }

  return { fit }
}
