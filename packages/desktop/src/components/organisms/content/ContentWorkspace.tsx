import { useEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { useGroupRef, type LayoutChangedMeta } from "react-resizable-panels"
import { useShallow } from "zustand/react/shallow"
import { useAppStore } from "../../../stores/appStore"
import { CHAT_MIN_WIDTH } from "../../../stores/layoutConstants"
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
import {
  CONTENT_LEFT_PANEL_ID as LEFT_PANEL_ID,
  CONTENT_RIGHT_PANEL_ID as RIGHT_PANEL_ID,
  contentWorkspaceDefaultLayout,
} from "./contentWorkspaceLayout"

export { contentWorkspaceDefaultLayout } from "./contentWorkspaceLayout"

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
      : tab.kind === "file"
        ? tab.path.split(/[/\\]/).pop() || tab.path
        : tab.tool.charAt(0).toUpperCase() + tab.tool.slice(1)
  return createPortal(
    <div
      aria-hidden
      className={cn(
        "pointer-events-none fixed z-[var(--z-overlay)] flex h-6 max-w-[180px] items-center truncate rounded-md",
        "bg-panel px-2 text-sm tracking-[var(--tracking-caption)] text-ink shadow-popover",
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

export const ContentWorkspace = ({
  onOpenCommandPalette,
  onOpenSearch,
}: {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
} = {}) => {
  useContentTabLifecycle()
  useInstallContentTabPointerDnD()
  // Coarse field selectors — avoid re-render on every contentLayout nested write.
  const mode = useAppStore((s) => s.contentLayout.mode)
  const splitRatio = useAppStore((s) => s.contentLayout.splitRatio)
  const paneCount = useAppStore((s) => s.contentLayout.panes.length)
  const setSplitRatio = useAppStore((s) => s.setSplitRatio)
  const setRightPanelDragging = useAppStore((s) => s.setRightPanelDragging)
  const rightPanelDragging = useAppStore((s) => s.rightPanelDragging)
  const keepAliveToolKeys = useAppStore(
    useShallow((s) => {
      const keys: string[] = []
      for (const pane of s.contentLayout.panes) {
        for (const t of pane.tabs) {
          if (
            t.kind === "tool" &&
            (t.tool === "files" ||
              t.tool === "terminal" ||
              t.tool === "browser")
          ) {
            keys.push(`${t.sessionId}:${t.tool}`)
          }
        }
      }
      return keys
    }),
  )
  const groupImperativeRef = useGroupRef()
  const containerRef = useRef<HTMLDivElement | null>(null)
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

  const keepAliveTools = useMemo(
    () => new Set(keepAliveToolKeys),
    [keepAliveToolKeys],
  )

  const split = mode === "split" && paneCount > 1
  const showSash = split && canShowSash

  const defaultLayout = useMemo(
    () => contentWorkspaceDefaultLayout(split, splitRatio),
    [split, splitRatio],
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
          <ContentPane
            paneIndex={0}
            keepAliveTools={keepAliveTools}
            isEastmost={!split}
            onOpenCommandPalette={onOpenCommandPalette}
            onOpenSearch={onOpenSearch}
          />
        </ResizablePanel>
        {split ? (
          <>
            <ResizableHandle
              disabled={!showSash}
              aria-label="Resize content panes"
              className={cn(
                "sash-line-transition z-20 w-2 shrink-0 bg-transparent",
                // Hit target extends into the chat pane (never under the
                // native Browser webview, which paints above every DOM stack).
                "before:absolute before:inset-y-0 before:-left-2 before:w-4 before:content-['']",
                "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-stroke-3",
                showSash && "cursor-col-resize",
                showSash &&
                  "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_12%,transparent)]",
                "focus-visible:ring-1 focus-visible:ring-stroke-2 focus-visible:outline-none",
                rightPanelDragging && "after:bg-stroke-1",
                !showSash && "pointer-events-none",
              )}
              onPointerDown={(e) => {
                if (!showSash) return
                e.currentTarget.setPointerCapture?.(e.pointerId)
                // Hide native Browser webview for the drag — it sits above DOM
                // and would otherwise eat pointermove / block the sash.
                setRightPanelDragging(true)
              }}
              onPointerUp={() => setRightPanelDragging(false)}
              onPointerCancel={() => setRightPanelDragging(false)}
            />
            <ResizablePanel
              id={RIGHT_PANEL_ID}
              minSize={CHAT_MIN_WIDTH}
              className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden"
            >
              <ContentPane
                paneIndex={1}
                keepAliveTools={keepAliveTools}
                isEastmost
                onOpenCommandPalette={onOpenCommandPalette}
                onOpenSearch={onOpenSearch}
              />
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
    </div>
  )
}
