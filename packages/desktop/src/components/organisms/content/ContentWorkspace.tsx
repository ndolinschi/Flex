import { useMemo, useRef } from "react"
import { useAppStore } from "../../../stores/appStore"
import { CHAT_MIN_WIDTH } from "../../../stores/layoutConstants"
import { AppHeader } from "../AppHeader"
import { ContentPane } from "./ContentPane"
import { useContentTabLifecycle } from "../../../hooks/useContentTabLifecycle"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { cn } from "@/lib/utils"

/** Main content host: AppHeader + one or two ContentPanes with a sash. */
export const ContentWorkspace = () => {
  useContentTabLifecycle()
  const contentLayout = useAppStore((s) => s.contentLayout)
  const setSplitRatio = useAppStore((s) => s.setSplitRatio)
  const setRightPanelDragging = useAppStore((s) => s.setRightPanelDragging)
  const viewport = useAppStore((s) => s.viewport)
  const draggingRef = useRef(false)

  const keepAliveTools = useMemo(() => {
    const set = new Set<string>()
    for (const pane of contentLayout.panes) {
      for (const t of pane.tabs) {
        if (
          t.kind === "tool" &&
          (t.tool === "files" ||
            t.tool === "terminal" ||
            t.tool === "browser" ||
            t.tool === "prompt")
        ) {
          set.add(`${t.sessionId}:${t.tool}`)
        }
      }
    }
    return set
  }, [contentLayout.panes])

  const split =
    contentLayout.mode === "split" &&
    viewport === "wide" &&
    contentLayout.panes.length > 1

  const defaultLayout = useMemo(
    () => ({
      "content-pane-0": contentLayout.splitRatio * 100,
      "content-pane-1": (1 - contentLayout.splitRatio) * 100,
    }),
    // Only seed when entering split — drag updates the store separately.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [split],
  )

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
      <AppHeader />
      {split ? (
        <ResizablePanelGroup
          id="content-workspace"
          orientation="horizontal"
          className="relative min-h-0 min-w-0 flex-1"
          defaultLayout={defaultLayout}
          onLayoutChange={(layout) => {
            const left = layout["content-pane-0"]
            if (typeof left !== "number") return
            if (!draggingRef.current) {
              draggingRef.current = true
              setRightPanelDragging(true)
            }
            setSplitRatio(left / 100, false)
          }}
          onLayoutChanged={(layout) => {
            const left = layout["content-pane-0"]
            draggingRef.current = false
            setRightPanelDragging(false)
            if (typeof left === "number") {
              setSplitRatio(left / 100, true)
            }
          }}
        >
          <ResizablePanel
            id="content-pane-0"
            minSize={CHAT_MIN_WIDTH}
            className="flex min-h-0 min-w-0 flex-col overflow-hidden"
          >
            <ContentPane paneIndex={0} keepAliveTools={keepAliveTools} />
          </ResizablePanel>
          <ResizableHandle
            aria-label="Resize content panes"
            className={cn(
              "sash-line-transition relative z-10 w-1.5 bg-transparent",
              "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-stroke-3 after:translate-x-0",
              "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_15%,transparent)]",
              "data-[separator=active]:after:bg-stroke-1",
            )}
          />
          <ResizablePanel
            id="content-pane-1"
            minSize={CHAT_MIN_WIDTH}
            className="flex min-h-0 min-w-0 flex-col overflow-hidden"
          >
            <ContentPane paneIndex={1} keepAliveTools={keepAliveTools} />
          </ResizablePanel>
        </ResizablePanelGroup>
      ) : (
        <div className="relative flex min-h-0 min-w-0 flex-1">
          <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
            <ContentPane paneIndex={0} keepAliveTools={keepAliveTools} />
          </div>
        </div>
      )}
    </div>
  )
}
