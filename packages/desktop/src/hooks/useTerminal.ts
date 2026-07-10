import { useEffect, useRef, type RefObject } from "react"
import { Terminal, type ITheme } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import "@xterm/xterm/css/xterm.css"
import { terminalResize, terminalWrite } from "../lib/tauri"
import { subscribeTerminal } from "../lib/terminalBus"

/** Read live CSS custom properties so the terminal tracks theme switches. */
const readThemeVars = (): ITheme => {
  const styles = getComputedStyle(document.documentElement)
  const read = (name: string, fallback: string) => {
    const value = styles.getPropertyValue(name).trim()
    return value || fallback
  }

  return {
    background: read("--color-chrome", "#0c0e11"),
    foreground: read("--color-ink", "#eef2fa"),
    cursor: read("--color-ink", "#eef2fa"),
    cursorAccent: read("--color-chrome", "#0c0e11"),
    selectionBackground: read("--color-fill-2", "rgb(255 255 255 / 0.07)"),
  }
}

/**
 * Owns one xterm.js `Terminal` instance bound to a backend PTY session `id`.
 * Mount one `useTerminal` per rendered terminal container; the hook wires
 * input/output/resize/exit and disposes everything on unmount.
 */
export const useTerminal = (
  id: string,
  containerRef: RefObject<HTMLDivElement | null>,
) => {
  const fitRef = useRef<FitAddon | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const term = new Terminal({
      fontFamily: 'Menlo, Monaco, ui-monospace, "SF Mono", monospace',
      fontSize: 12,
      lineHeight: 1.4,
      cursorBlink: true,
      scrollback: 5000,
      theme: readThemeVars(),
      allowProposedApi: false,
    })

    const fitAddon = new FitAddon()
    fitRef.current = fitAddon
    term.loadAddon(fitAddon)

    term.open(container)
    if (container.clientWidth > 0 && container.clientHeight > 0) {
      fitAddon.fit()
    }

    const dataDisposable = term.onData((data) => {
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
      void terminalResize(id, term.cols, term.rows)
    })
    resizeObserver.observe(container)

    // Synchronous subscribe via the bus: replays buffered scrollback first,
    // so output emitted before this instance mounted (shell prompt, StrictMode
    // remount gaps) is never lost.
    const unsubscribe = subscribeTerminal(id, (data) => term.write(data))

    return () => {
      themeObserver.disconnect()
      resizeObserver.disconnect()
      dataDisposable.dispose()
      unsubscribe()
      fitRef.current = null
      term.dispose()
    }
  }, [id, containerRef])

  const fit = () => {
    const container = containerRef.current
    if (!container) return
    if (container.clientWidth === 0 || container.clientHeight === 0) return
    fitRef.current?.fit()
  }

  return { fit }
}
