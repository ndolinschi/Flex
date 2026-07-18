import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type WheelEvent as ReactWheelEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { MessageSquare, Plus, X } from "lucide-react"
import { IconButton, Tab, TabStrip, Tooltip } from "../../atoms"
import { OpenTabModal } from "../../molecules"
import { useSessions } from "../../../hooks/useSessions"
import {
  startContentTabPointerDrag,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"
import { gitPrStatus } from "../../../lib/tauri"
import { sessionLabel, type SessionId } from "../../../lib/types"
import {
  useAppStore,
  type ContentTab,
  type RightPanelTab,
} from "../../../stores/appStore"
import { visibleRightPanelTabs } from "../right-panel/tabs"
import { ChatSessionBody } from "./ChatSessionBody"
import { ToolTabBody } from "./ToolTabBody"
import { cn } from "../../../lib/utils"

type ContentPaneProps = {
  paneIndex: 0 | 1
  /** Tool tabs that must stay mounted (browser/terminal/files) across panes. */
  keepAliveTools: Set<string>
}

/** Labels/icons for the strip — always include PR so open tabs keep a label. */
const STRIP_CATALOG = visibleRightPanelTabs({ hasBranchPr: true })

const EMPTY_PANE = { tabs: [] as ContentTab[], activeTabId: null as string | null }

const tabLabel = (
  tab: ContentTab,
  sessionsById: Map<string, { id: string; title?: string | null }>,
): string => {
  if (tab.kind === "chat") {
    const s = sessionsById.get(tab.sessionId)
    return s ? sessionLabel(s as never) : "Chat"
  }
  return STRIP_CATALOG.find((c) => c.id === tab.tool)?.label ?? tab.tool
}

export const ContentPane = ({ paneIndex, keepAliveTools }: ContentPaneProps) => {
  // Narrow selectors — avoid re-rendering this pane when only the sibling
  // pane's tabs change (structural sharing in activate/reorder/close).
  const pane = useAppStore(
    (s) => s.contentLayout.panes[paneIndex] ?? EMPTY_PANE,
  )
  const split = useAppStore((s) => s.contentLayout.mode === "split")
  const focusedPane = useAppStore((s) => s.contentLayout.focusedPane)
  const activateTabInPane = useAppStore((s) => s.activateTabInPane)
  const closeTabInPane = useAppStore((s) => s.closeTabInPane)
  const closePane = useAppStore((s) => s.closePane)
  const openChatInPane = useAppStore((s) => s.openChatInPane)
  const openToolInPane = useAppStore((s) => s.openToolInPane)
  const setFocusedPane = useAppStore((s) => s.setFocusedPane)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const { sessions } = useSessions()
  const dragUi = useTabDragUi()
  const [openTabModal, setOpenTabModal] = useState(false)
  const [openTabAnchor, setOpenTabAnchor] = useState<{
    x: number
    y: number
    width: number
    height: number
  } | null>(null)
  const tabsScrollRef = useRef<HTMLDivElement>(null)
  /** Chat bodies stay mounted after first visit (scroll/draft locality). */
  const visitedChatsRef = useRef(new Set<string>())
  const [, bumpVisited] = useState(0)

  const sessionsById = useMemo(
    () => new Map(sessions.map((s) => [s.id, s])),
    [sessions],
  )

  const contextSession: SessionId | null = (() => {
    const active = pane.tabs.find((t) => t.id === pane.activeTabId)
    if (active) return active.sessionId
    return activeSessionId
  })()

  const cwd = sessions.find((s) => s.id === contextSession)?.cwd
  // Only fetch PR status while the + menu is open — strip labels never need it.
  const prQuery = useQuery({
    queryKey: ["git-pr-status", cwd ?? ""],
    queryFn: () => gitPrStatus(cwd!),
    enabled: !!cwd && openTabModal,
    staleTime: 15_000,
  })
  const hasBranchPr = !!prQuery.data?.pr
  const catalog = useMemo(
    () => visibleRightPanelTabs({ hasBranchPr }),
    [hasBranchPr],
  )
  const paneFocused = focusedPane === paneIndex

  // Vertical wheel → horizontal scroll over the tab strip (trackpad/mouse).
  const handleTabsWheel = (e: ReactWheelEvent<HTMLDivElement>) => {
    const el = tabsScrollRef.current
    if (!el) return
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
    if (el.scrollWidth <= el.clientWidth) return
    e.preventDefault()
    el.scrollLeft += e.deltaY
  }

  // Keep the active tab visible when it changes or tabs are added.
  useEffect(() => {
    const id = pane.activeTabId
    if (!id) return
    const el = tabsScrollRef.current?.querySelector<HTMLElement>(
      `[data-tab-id="${CSS.escape(id)}"]`,
    )
    el?.scrollIntoView({ block: "nearest", inline: "nearest" })
  }, [pane.activeTabId, pane.tabs.length])

  // Mark the active chat as visited so we keep its body mounted when hidden.
  useEffect(() => {
    const active = pane.tabs.find((t) => t.id === pane.activeTabId)
    if (active?.kind !== "chat") return
    if (visitedChatsRef.current.has(active.id)) return
    visitedChatsRef.current.add(active.id)
    bumpVisited((n) => n + 1)
  }, [pane.activeTabId, pane.tabs])

  // Drop visit marks for tabs that no longer exist in this pane.
  useEffect(() => {
    const ids = new Set(pane.tabs.map((t) => t.id))
    let changed = false
    for (const id of visitedChatsRef.current) {
      if (!ids.has(id)) {
        visitedChatsRef.current.delete(id)
        changed = true
      }
    }
    if (changed) bumpVisited((n) => n + 1)
  }, [pane.tabs])

  const dragTabId = dragUi?.dragging ? dragUi.tabId : null
  const dropInsertAt =
    dragUi?.dragging && dragUi.toPane === paneIndex ? dragUi.insertAt : null

  return (
    <div
      className={cn(
        "relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-bg",
        paneFocused ? "z-[1]" : "z-0",
      )}
      onMouseDown={() => setFocusedPane(paneIndex)}
    >
      <TabStrip
        aria-label={paneIndex === 0 ? "Left pane tabs" : "Right pane tabs"}
        className="min-w-0 gap-1"
      >
        <div
          ref={tabsScrollRef}
          role="presentation"
          data-content-tab-strip={paneIndex}
          onWheel={handleTabsWheel}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto",
            "[scrollbar-width:none] [&::-webkit-scrollbar]:hidden",
          )}
        >
          {pane.tabs.map((t, index) => {
            const def =
              t.kind === "tool"
                ? STRIP_CATALOG.find((c) => c.id === t.tool)
                : undefined
            const dropEdge =
              dragTabId && dropInsertAt != null && dragTabId !== t.id
                ? dropInsertAt === index
                  ? "before"
                  : dropInsertAt === index + 1
                    ? "after"
                    : null
                : null
            const label = tabLabel(t, sessionsById)
            return (
              <Tab
                key={t.id}
                selected={t.id === pane.activeTabId}
                icon={
                  t.kind === "chat" ? (
                    <MessageSquare aria-hidden />
                  ) : def?.icon ? (
                    <def.icon aria-hidden />
                  ) : undefined
                }
                className={cn(
                  "max-w-[180px] shrink-0",
                  dragTabId === t.id && "opacity-40",
                )}
                title={label}
                tabId={t.id}
                onSelect={() => activateTabInPane(paneIndex, t.id)}
                onClose={() => closeTabInPane(paneIndex, t.id)}
                closeLabel={`Close ${label}`}
                draggable
                dropEdge={dropEdge}
                onPointerDown={(e) =>
                  startContentTabPointerDrag(e, paneIndex, t.id)
                }
              >
                {label}
              </Tab>
            )
          })}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <IconButton
            label="Open tab"
            onClick={(e: ReactMouseEvent<HTMLButtonElement>) => {
              const r = e.currentTarget.getBoundingClientRect()
              setOpenTabAnchor({
                x: r.left,
                y: r.top,
                width: r.width,
                height: r.height,
              })
              setOpenTabModal(true)
            }}
            className="h-6 w-6"
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
          {split ? (
            <Tooltip label="Close pane">
              <IconButton
                label="Close pane"
                onClick={(e: ReactMouseEvent<HTMLButtonElement>) => {
                  e.stopPropagation()
                  closePane(paneIndex)
                }}
                className="h-6 w-6"
                quiet
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </IconButton>
            </Tooltip>
          ) : null}
        </div>
      </TabStrip>

      <div className="relative min-h-0 flex-1">
        {pane.tabs.map((t) => {
          const isActive = t.id === pane.activeTabId
          if (t.kind === "chat") {
            const mount =
              isActive || visitedChatsRef.current.has(t.id)
            if (!mount) return null
            return (
              <div
                key={t.id}
                className={cn(
                  "absolute inset-0 flex flex-col",
                  isActive ? "flex" : "hidden",
                )}
              >
                <ChatSessionBody
                  sessionId={t.sessionId}
                  active={isActive && paneFocused}
                />
              </div>
            )
          }
          const keepKey = `${t.sessionId}:${t.tool}`
          return (
            <ToolTabBody
              key={t.id}
              tool={t.tool}
              session={sessions.find((s) => s.id === t.sessionId)}
              active={isActive}
              keepAlive={keepAliveTools.has(keepKey)}
            />
          )
        })}
        {pane.tabs.length === 0 ? (
          <div className="flex h-full items-center justify-center px-2.5 text-sm text-ink-muted">
            Open a chat or tool tab with +
          </div>
        ) : null}
      </div>

      <OpenTabModal
        open={openTabModal}
        onClose={() => {
          setOpenTabModal(false)
          setOpenTabAnchor(null)
        }}
        anchor={openTabAnchor}
        paneIndex={paneIndex}
        sessionId={contextSession}
        tabs={catalog}
        onOpenChat={openChatInPane}
        onOpenTool={(pane, sid, tool) =>
          openToolInPane(pane, sid, tool as RightPanelTab)
        }
      />
    </div>
  )
}
