import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { createPortal } from "react-dom"
import { useAppStore } from "../../../stores/appStore"
import { CHAT_MIN_WIDTH } from "../../../stores/layoutConstants"
import { clampSplitRatio } from "../../../stores/contentLayoutModel"
import { ContentPane } from "./ContentPane"
import { cn } from "../../../lib/utils"
import { useContentTabLifecycle } from "../../../hooks/useContentTabLifecycle"
import {
  useInstallContentTabPointerDnD,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"

/** Floating label that follows the pointer while a tab is dragged. */
const TabDragGhost = () => {
  const dragUi = useTabDragUi()
  const contentLayout = useAppStore((s) => s.contentLayout)
  if (!dragUi?.dragging) return null
  const source = contentLayout.panes[dragUi.fromPane]
  const tab = source?.tabs.find((t) => t.id === dragUi.tabId)
  if (!tab) return null
  const label =
    tab.kind === "chat"
      ? "Chat"
      : tab.tool.charAt(0).toUpperCase() + tab.tool.slice(1)
  return createPortal(
    <div
      aria-hidden
      className={cn(
        "pointer-events-none fixed z-[9999] h-6 max-w-[180px] truncate rounded-md",
        "border border-stroke-2 bg-elevated px-2 text-sm text-ink shadow-md",
        "opacity-90",
        !dragUi.overTarget && "opacity-40",
      )}
      style={{
        left: dragUi.pointerX + 12,
        top: dragUi.pointerY + 8,
      }}
    >
      {label}
    </div>,
    document.body,
  )
}

/** Main content host: one or two ContentPanes with a sash.
 * Chat chrome (sidebar / split / session) lives on WindowTitleBar. */
export const ContentWorkspace = () => {
  useContentTabLifecycle()
  useInstallContentTabPointerDnD()
  const contentLayout = useAppStore((s) => s.contentLayout)
  const setSplitRatio = useAppStore((s) => s.setSplitRatio)
  const setRightPanelDragging = useAppStore((s) => s.setRightPanelDragging)
  const [dragging, setDragging] = useState(false)
  const rowRef = useRef<HTMLDivElement>(null)
  // Threshold boolean — avoid re-rendering on every resize pixel while the
  // sash eligibility does not change.
  const [canShowSash, setCanShowSash] = useState(false)

  useEffect(() => {
    const el = rowRef.current
    if (!el) return
    const update = (width: number) => {
      const next = width >= CHAT_MIN_WIDTH * 2
      setCanShowSash((prev) => (prev === next ? prev : next))
    }
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (entry) update(entry.contentRect.width)
    })
    ro.observe(el)
    update(el.getBoundingClientRect().width)
    return () => ro.disconnect()
  }, [])

  const keepAliveTools = useMemo(() => {
    const set = new Set<string>()
    for (const pane of contentLayout.panes) {
      for (const t of pane.tabs) {
        // Prompt is cheap to remount and otherwise re-renders on every
        // composer keystroke via draftsBySession while keep-alive hidden.
        if (
          t.kind === "tool" &&
          (t.tool === "files" || t.tool === "terminal" || t.tool === "browser")
        ) {
          set.add(`${t.sessionId}:${t.tool}`)
        }
      }
    }
    return set
  }, [contentLayout.panes])

  const split = contentLayout.mode === "split" && contentLayout.panes.length > 1
  // Only render the resize sash when the content row is wide enough for two
  // minimum-width panes. Both panes stay mounted to preserve scroll/xterm
  // state; the sash handle just hides when there is no room to drag it.
  const showSash = split && canShowSash

  const handleSashDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault()
    const row = rowRef.current
    if (!row) return
    setDragging(true)
    setRightPanelDragging(true)
    const startX = e.clientX
    const startRatio = contentLayout.splitRatio
    const width = row.getBoundingClientRect().width

    const onMove = (ev: globalThis.PointerEvent) => {
      if (width <= 0) return
      const delta = (ev.clientX - startX) / width
      const minRatio = CHAT_MIN_WIDTH / width
      const maxRatio = 1 - minRatio
      const next = clampSplitRatio(
        Math.min(maxRatio, Math.max(minRatio, startRatio + delta)),
      )
      setSplitRatio(next, false)
    }
    const onUp = () => {
      setDragging(false)
      setRightPanelDragging(false)
      setSplitRatio(useAppStore.getState().contentLayout.splitRatio, true)
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
      <TabDragGhost />
      <div
        ref={rowRef}
        className="relative flex min-h-0 min-w-0 flex-1"
      >
        <div
          className="flex min-h-0 min-w-0 flex-col overflow-hidden"
          style={
            split
              ? { width: `${contentLayout.splitRatio * 100}%`, flex: "none" }
              : { flex: 1 }
          }
        >
          <ContentPane paneIndex={0} keepAliveTools={keepAliveTools} />
        </div>
        {split ? (
          <>
            {showSash ? (
              <div
                role="separator"
                aria-orientation="vertical"
                aria-label="Resize content panes"
                tabIndex={0}
                onPointerDown={handleSashDown}
                className={cn(
                  "sash-line-transition relative z-10 w-1.5 shrink-0 cursor-col-resize",
                  "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-stroke-3",
                  "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_15%,transparent)]",
                  dragging && "after:bg-stroke-1",
                )}
              />
            ) : null}
            <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
              <ContentPane paneIndex={1} keepAliveTools={keepAliveTools} />
            </div>
          </>
        ) : null}
      </div>
    </div>
  )
}
