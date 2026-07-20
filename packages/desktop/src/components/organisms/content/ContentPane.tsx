import { Button } from "@/components/ui/button"
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type WheelEvent as ReactWheelEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { MessageSquare, Plus, X } from "lucide-react"
import { Tab, TabStrip, Tooltip } from "../../atoms"
import { ContextMenu, OpenTabModal, type ContextMenuItem } from "../../molecules"
import { useSessions } from "../../../hooks/useSessions"
import {
  startContentTabPointerDrag,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"
import { previewTabsForPane } from "../../../lib/tabDnD"
import { gitPrStatus } from "../../../lib/tauri"
import { sessionLabel, type SessionId, type SessionMeta } from "../../../lib/types"
import {
  useAppStore,
  type ContentTab,
  type RightPanelTab,
} from "../../../stores/appStore"
import { emptyPane } from "../../../stores/contentLayoutModel"
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

/** Stable fallback when a pane index is missing (must keep object identity). */
const EMPTY_PANE = emptyPane()

const tabLabel = (
  tab: ContentTab,
  sessionsById: Map<string, SessionMeta>,
): string => {
  if (tab.kind === "chat") {
    const s = sessionsById.get(tab.sessionId)
    return s ? sessionLabel(s) : "Chat"
  }
  return STRIP_CATALOG.find((c) => c.id === tab.tool)?.label ?? tab.tool
}

/**
 * Track which chat tab ids have been shown so we can keep their bodies mounted
 * after the first visit (scroll/draft locality) without mounting every open chat.
 */
const useVisitedChatTabs = (
  tabs: ContentTab[],
  activeTabId: string | null,
): ReadonlySet<string> => {
  const [visited, setVisited] = useState<ReadonlySet<string>>(() => new Set())

  useEffect(() => {
    setVisited((prev) => {
      const openIds = new Set(tabs.map((t) => t.id))
      let next: Set<string> | null = null

      const active = tabs.find((t) => t.id === activeTabId)
      if (active?.kind === "chat" && !prev.has(active.id)) {
        next = new Set(prev)
        next.add(active.id)
      }

      for (const id of prev) {
        if (!openIds.has(id)) {
          if (!next) next = new Set(prev)
          next.delete(id)
        }
      }

      return next ?? prev
    })
  }, [activeTabId, tabs])

  return visited
}

/** Build a single CSS mask-image with optional left/right edge fades. */
const buildScrollMask = (left: boolean, right: boolean): string | undefined => {
  if (!left && !right) return undefined
  const start = left ? "transparent 0px, black 20px" : "black 0px"
  const end = right ? "black calc(100% - 20px), transparent 100%" : "black 100%"
  return `linear-gradient(to right, ${start}, ${end})`
}

export const ContentPane = ({ paneIndex, keepAliveTools }: ContentPaneProps) => {
  // Narrow selectors — avoid re-rendering this pane when only the sibling
  // pane's tabs change (structural sharing in activate/reorder/close).
  const pane = useAppStore(
    (s) => s.contentLayout.panes[paneIndex] ?? EMPTY_PANE,
  )
  const split = useAppStore((s) => s.contentLayout.mode === "split")
  const focusedPane = useAppStore((s) => s.contentLayout.focusedPane)
  const viewport = useAppStore((s) => s.viewport)
  const activateTabInPane = useAppStore((s) => s.activateTabInPane)
  const closeTabInPane = useAppStore((s) => s.closeTabInPane)
  const closeOtherTabsInPane = useAppStore((s) => s.closeOtherTabsInPane)
  const closeTabsToRightInPane = useAppStore((s) => s.closeTabsToRightInPane)
  const closePane = useAppStore((s) => s.closePane)
  const openChatInPane = useAppStore((s) => s.openChatInPane)
  const openToolInPane = useAppStore((s) => s.openToolInPane)
  const openTabToSide = useAppStore((s) => s.openTabToSide)
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
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null)
  const [menuTabId, setMenuTabId] = useState<string | null>(null)
  const [scrollFade, setScrollFade] = useState({ left: false, right: false })
  const tabsScrollRef = useRef<HTMLDivElement>(null)
  const visitedChats = useVisitedChatTabs(pane.tabs, pane.activeTabId)

  const sessionsById = useMemo(
    () => new Map(sessions.map((s) => [s.id, s])),
    [sessions],
  )

  // True when tabs from more than one session are open in this pane.
  const hasMultipleSessions = useMemo(() => {
    const ids = new Set(pane.tabs.map((t) => t.sessionId))
    return ids.size > 1
  }, [pane.tabs])

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

  // Edge fade: track whether there is overflow on left/right of the tab strip.
  const updateScrollFade = useCallback(() => {
    const el = tabsScrollRef.current
    if (!el) return
    setScrollFade({
      left: el.scrollLeft > 1,
      right: el.scrollLeft + el.clientWidth < el.scrollWidth - 1,
    })
  }, [])

  useEffect(() => {
    const el = tabsScrollRef.current
    if (!el) return
    const ro = new ResizeObserver(updateScrollFade)
    ro.observe(el)
    el.addEventListener("scroll", updateScrollFade, { passive: true })
    updateScrollFade()
    return () => {
      ro.disconnect()
      el.removeEventListener("scroll", updateScrollFade)
    }
  }, [updateScrollFade])

  // Re-check fade whenever the tab count changes (scrollWidth may change).
  useEffect(() => {
    updateScrollFade()
  }, [pane.tabs.length, updateScrollFade])

  // Arrow-key focus navigation within the tab strip (roving tabIndex).
  const handleTabsKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return
      const el = tabsScrollRef.current
      if (!el) return
      const tabs = Array.from(
        el.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
      )
      const idx = tabs.indexOf(document.activeElement as HTMLButtonElement)
      if (idx === -1) return
      e.preventDefault()
      const next =
        e.key === "ArrowLeft" ? tabs[idx - 1] : tabs[idx + 1]
      next?.focus()
    },
    [],
  )

  const dragTabId = dragUi?.dragging ? dragUi.tabId : null
  const sourceTabs = useMemo(() => {
    if (!dragUi?.dragging) return pane.tabs
    if (dragUi.fromPane === paneIndex) return pane.tabs
    return (
      useAppStore.getState().contentLayout.panes[dragUi.fromPane]?.tabs ??
      pane.tabs
    )
  }, [dragUi, pane.tabs, paneIndex])
  const displayTabs = previewTabsForPane(
    paneIndex,
    pane.tabs,
    sourceTabs,
    dragUi,
  )
  const dropInsertAt =
    dragUi?.dragging && dragUi.overTarget && dragUi.toPane === paneIndex
      ? dragUi.insertAt
      : null

  // Context menu items for the right-clicked tab.
  const contextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!menuTabId) return []
    const idx = pane.tabs.findIndex((t) => t.id === menuTabId)
    const menuTab = idx >= 0 ? pane.tabs[idx] : undefined
    // Browser holds a singleton native webview — duplicating across panes races.
    const openToSideDisabled =
      viewport !== "wide" ||
      (menuTab?.kind === "tool" && menuTab.tool === "browser")
    return [
      {
        type: "item",
        label: "Open to Side",
        disabled: openToSideDisabled,
        onSelect: () => openTabToSide(paneIndex, menuTabId),
      },
      { type: "separator" },
      {
        type: "item",
        label: "Close",
        onSelect: () => closeTabInPane(paneIndex, menuTabId),
      },
      {
        type: "item",
        label: "Close Others",
        disabled: pane.tabs.length <= 1,
        onSelect: () => closeOtherTabsInPane(paneIndex, menuTabId),
      },
      {
        type: "item",
        label: "Close to Right",
        disabled: idx < 0 || idx >= pane.tabs.length - 1,
        onSelect: () => closeTabsToRightInPane(paneIndex, menuTabId),
      },
    ]
  }, [
    menuTabId,
    pane.tabs,
    paneIndex,
    viewport,
    openTabToSide,
    closeTabInPane,
    closeOtherTabsInPane,
    closeTabsToRightInPane,
  ])

  const scrollMask = buildScrollMask(scrollFade.left, scrollFade.right)

  return (
    <div
      className={cn(
        "relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-bg",
        paneFocused ? "z-[1]" : "z-0",
      )}
      data-content-pane={paneIndex}
      onMouseDown={() => setFocusedPane(paneIndex)}
    >
      <TabStrip
        aria-label={paneIndex === 0 ? "Left pane tabs" : "Right pane tabs"}
        className="min-w-0"
        onKeyDown={handleTabsKeyDown}
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
          style={
            scrollMask
              ? { WebkitMaskImage: scrollMask, maskImage: scrollMask }
              : undefined
          }
        >
          {displayTabs.map((t, index) => {
            const def =
              t.kind === "tool"
                ? STRIP_CATALOG.find((c) => c.id === t.tool)
                : undefined
            const isDragged = dragTabId === t.id
            const dropEdge =
              dropInsertAt != null
                ? dropInsertAt === index
                  ? "before"
                  : dropInsertAt >= displayTabs.length &&
                      index === displayTabs.length - 1
                    ? "after"
                    : null
                : null
            const label = tabLabel(t, sessionsById)

            // Session ownership cue: when multiple sessions share a pane,
            // suffix tool-tab titles with the owning session label.
            const titleText =
              hasMultipleSessions && t.kind === "tool"
                ? `${label} — ${sessionLabel(sessionsById.get(t.sessionId) ?? { title: t.sessionId.slice(0, 8) } as SessionMeta)}`
                : label

            // Divider between the last chat tab and first tool tab.
            const prev = displayTabs[index - 1]
            const showDivider = prev?.kind === "chat" && t.kind === "tool"

            return (
              <Fragment key={t.id}>
                {showDivider ? (
                  <span
                    aria-hidden
                    className="mx-0.5 h-4 w-px shrink-0 bg-stroke-3"
                  />
                ) : null}
                <Tab
                  selected={t.id === pane.activeTabId}
                  icon={
                    t.kind === "chat" ? (
                      <MessageSquare aria-hidden />
                    ) : def?.icon ? (
                      <def.icon aria-hidden />
                    ) : undefined
                  }
                  className={cn(
                    "max-w-[180px] shrink-0 transition-[opacity,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                    isDragged && "opacity-40",
                  )}
                  title={titleText}
                  tabId={t.id}
                  tabIndex={t.id === pane.activeTabId ? 0 : -1}
                  onSelect={() => activateTabInPane(paneIndex, t.id)}
                  onClose={() => closeTabInPane(paneIndex, t.id)}
                  onContextMenu={(e) => {
                    e.preventDefault()
                    setMenuTabId(t.id)
                    setMenuPosition({ x: e.clientX, y: e.clientY })
                  }}
                  closeLabel={`Close ${label}`}
                  draggable
                  dropEdge={dropEdge}
                  onPointerDown={(e) =>
                    startContentTabPointerDrag(e, paneIndex, t.id)
                  }
                >
                  {label}
                </Tab>
              </Fragment>
            )
          })}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label="Open tab" title="Open tab"
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
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      <Plus aria-hidden />
    </Button>
          {split ? (
            <Tooltip label="Close pane">
              <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Close pane" title="Close pane"
      onClick={(e: ReactMouseEvent<HTMLButtonElement>) => {
                  e.stopPropagation()
                  closePane(paneIndex)
                }}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "opacity-50 hover:opacity-80",
        "h-6 w-6",
      )}
    >
      <X className="h-3.5 w-3.5" aria-hidden />
    </Button>
            </Tooltip>
          ) : null}
        </div>
      </TabStrip>

      <div className="relative min-h-0 flex-1">
        {pane.tabs.map((t) => {
          const isActive = t.id === pane.activeTabId
          if (t.kind === "chat") {
            if (!isActive && !visitedChats.has(t.id)) return null
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
                  visible={isActive}
                  interactive={isActive && paneFocused}
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

      <ContextMenu
        position={menuPosition}
        items={contextMenuItems}
        onClose={() => {
          setMenuPosition(null)
          setMenuTabId(null)
        }}
      />

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
        onOpenTool={(p, sid, tool) =>
          openToolInPane(p, sid, tool as RightPanelTab)
        }
      />
    </div>
  )
}
