import { useEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { useGroupRef, type LayoutChangedMeta } from "react-resizable-panels"
import { useAppStore } from "../../../stores/appStore"
import { CHAT_MIN_WIDTH } from "../../../stores/layoutConstants"
import { clampSplitRatio } from "../../../stores/contentLayoutModel"
import { ContentPane } from "./ContentPane"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { cn } from "../../../lib/utils"
import { useContentTabLifecycle } from "../../../hooks/useContentTabLifecycle"
import {
  useInstallContentTabPointerDnD,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"

const LEFT_PANEL_ID = "content-left"
const RIGHT_PANEL_ID = "content-right"

/** Layout passed to `ResizablePanelGroup` for the current pane count.
 * Must match rendered panel count — a leftover `[50, 50]` under a single
 * panel throws `Invalid 1 panel layout: 50%, 50%`. */
export const contentWorkspaceDefaultLayout = (
  split: boolean,
  splitRatio: number,
): Record<string, number> => {
  if (!split) return { [LEFT_PANEL_ID]: 100 }
  const left = Math.round(clampSplitRatio(splitRatio) * 100)
  return {
    [LEFT_PANEL_ID]: left,
    [RIGHT_PANEL_ID]: 100 - left,
  }
}

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

/** Main content host: one or two ContentPanes with a resizable sash.
 * Chat chrome (sidebar / split / session) lives on WindowTitleBar. */
export const ContentWorkspace = () => {
  useContentTabLifecycle()
  useInstallContentTabPointerDnD()
  const contentLayout = useAppStore((s) => s.contentLayout)
  const setSplitRatio = useAppStore((s) => s.setSplitRatio)
  const setRightPanelDragging = useAppStore((s) => s.setRightPanelDragging)
  const groupImperativeRef = useGroupRef()
  const containerRef = useRef<HTMLDivElement | null>(null)
  // Threshold boolean — avoid re-rendering on every resize pixel while the
  // sash eligibility does not change.
  const [canShowSash, setCanShowSash] = useState(false)

  useEffect(() => {
    const el = containerRef.current
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
  // Only show the resize handle when the container is wide enough for two
  // minimum-width panes. Both panes stay mounted while split to preserve
  // scroll/xterm; the handle just hides when there is no room to drag it.
  const showSash = split && canShowSash

  // Snapshot sizes for the *current* panel count. Remount via `key` + matching
  // `defaultLayout` so collapsing split→single never validates a stale
  // two-size layout against one panel (see `contentWorkspaceDefaultLayout`).
  const defaultLayout = useMemo(
    () => contentWorkspaceDefaultLayout(split, contentLayout.splitRatio),
    // Ratio is snapshotted at mode change only — drag updates must not remount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [split],
  )

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
      <TabDragGhost />
      <ResizablePanelGroup
        key={split ? "split" : "single"}
        orientation="horizontal"
        elementRef={containerRef}
        groupRef={groupImperativeRef}
        defaultLayout={defaultLayout}
        onLayoutChange={(layout) => {
          if (!split) return
          const leftPct = layout[LEFT_PANEL_ID]
          if (leftPct === undefined) return
          setSplitRatio(leftPct / 100, false)
        }}
        onLayoutChanged={(
          layout: Record<string, number>,
          meta: LayoutChangedMeta,
        ) => {
          if (!split || !meta.isUserInteraction) return
          const leftPct = layout[LEFT_PANEL_ID]
          if (leftPct === undefined) return
          setRightPanelDragging(false)
          setSplitRatio(leftPct / 100, true)
        }}
        className="relative min-h-0 min-w-0 flex-1"
      >
        <ResizablePanel
          id={LEFT_PANEL_ID}
          minSize={CHAT_MIN_WIDTH}
          className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden"
        >
          <ContentPane paneIndex={0} keepAliveTools={keepAliveTools} />
        </ResizablePanel>
        {split ? (
          <>
            <ResizableHandle
              disabled={!showSash}
              className={cn(
                "sash-line-transition z-10 w-1.5 shrink-0 cursor-col-resize bg-transparent",
                "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-stroke-3",
                "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_12%,transparent)]",
                !showSash && "invisible pointer-events-none",
              )}
              onPointerDown={() => setRightPanelDragging(true)}
            />
            <ResizablePanel
              id={RIGHT_PANEL_ID}
              minSize={CHAT_MIN_WIDTH}
              className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden"
            >
              <ContentPane paneIndex={1} keepAliveTools={keepAliveTools} />
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
    </div>
  )
}
