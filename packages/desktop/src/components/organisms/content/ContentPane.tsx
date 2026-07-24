import { Button } from "@/components/ui/button"
import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { MessageSquare, X } from "lucide-react"
import { Tab, TabStrip, Tooltip } from "../../atoms"
import { ContextMenu, ErrorBanner, OpenTabModal } from "../../molecules"
import { useSessions } from "../../../hooks/useSessions"
import {
  startContentTabPointerDrag,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"
import { useTabStripScroll } from "../../../hooks/useTabStripScroll"
import { useContentPaneContextMenu } from "../../../hooks/useContentPaneContextMenu"
import {
  nextChatKeepAlive,
  sameStringList,
} from "../../../lib/chatKeepAlive"
import {
  activeChatTabId as resolveActiveChatTabId,
  openChatTabIds,
  shouldMountChatTab,
  shouldMountFileTab,
} from "../../../lib/chatMountPolicy"
import { previewTabsForPane } from "../../../lib/tabDnD"
import { sessionHasPlanReady } from "../../../lib/planReady"
import { gitPrStatus } from "../../../lib/tauri"
import { sessionLabel, type SessionId, type SessionMeta } from "../../../lib/types"
import {
  sessionScopeKey,
  useAppStore,
  type ContentTab,
  type RightPanelTab,
} from "../../../stores/appStore"
import { MAX_KEEPALIVE_CHAT_TABS } from "../../../stores/layoutConstants"
import { isSplitEligible } from "../../../stores/slices/contentLayoutSlice"
import { emptyPane } from "../../../stores/contentLayoutModel"
import {
  TitleBarDragRegion,
  TitleBarLeading,
  TitleBarTrailing,
} from "../TitleBarChrome"
import { ContextBar } from "../ContextBar"
import { visibleRightPanelTabs } from "../right-panel/tabs"
import { ChatSessionBody } from "./ChatSessionBody"
import { ToolTabBody } from "./ToolTabBody"
import { FileDocumentTab } from "../right-panel/FileDocumentTab"
import { PanelErrorBoundary } from "../../templates"
import { cn, basename, fileIconForPath } from "../../../lib/utils"
import { sessionColor, GROUP_PALETTE } from "../../../lib/sessionColor"
import { toggleZoomWindow } from "../../../lib/windowChrome"

type ContentPaneProps = {
  paneIndex: 0 | 1
  keepAliveTools: Set<string>
  isEastmost?: boolean
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
}

const fullStripCatalog = (hasPlanReady: boolean) =>
  visibleRightPanelTabs({ hasBranchPr: true, hasPlanReady })

const EMPTY_PANE = emptyPane()

type GroupSwatchBarProps = {
  onPickColor: (color: string) => void
}

const GroupSwatchBar = ({ onPickColor }: GroupSwatchBarProps) => (
  <div
    className="flex shrink-0 items-center gap-1 px-1"
    role="group"
    aria-label="Pick group color"
  >
    <span className="mr-0.5 text-xs text-ink-muted">Group:</span>
    {GROUP_PALETTE.map((color) => (
      <button
        key={color}
        type="button"
        aria-label={`Group with color ${color}`}
        title={`Group tabs — ${color}`}
        onClick={() => onPickColor(color)}
        className="h-3.5 w-3.5 shrink-0 rounded-full transition-transform duration-[var(--duration-fast)] ease-[var(--easing-default)] hover:scale-125 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2 motion-reduce:transition-none motion-reduce:hover:scale-100"
        style={{ backgroundColor: color }}
      />
    ))}
  </div>
)

const tabLabel = (
  tab: ContentTab,
  sessionsById: Map<string, SessionMeta>,
  catalog: ReturnType<typeof fullStripCatalog>,
): string => {
  if (tab.kind === "chat") {
    const s = sessionsById.get(tab.sessionId)
    return s ? sessionLabel(s) : "Chat"
  }
  if (tab.kind === "file") {
    return basename(tab.path)
  }
  return catalog.find((c) => c.id === tab.tool)?.label ?? tab.tool
}

/** LRU of visited chat tab ids kept mounted (hidden) for warm switch. */
const useChatKeepAliveTabs = (
  tabs: ContentTab[],
  activeTabId: string | null,
): ReadonlySet<string> => {
  // Seed from the current active chat so the first paint already mounts it
  // (and the previous active) — useEffect would leave a one-frame gap.
  const openIds = openChatTabIds(tabs)
  const activeChatId = resolveActiveChatTabId(tabs, activeTabId)
  const [kept, setKept] = useState<string[]>(() =>
    nextChatKeepAlive([], activeChatId, openIds, MAX_KEEPALIVE_CHAT_TABS),
  )

  // Sync during render when the active/open set changes so we never unmount
  // the newly active chat for a frame before useEffect runs.
  const desired = nextChatKeepAlive(
    kept,
    activeChatId,
    openIds,
    MAX_KEEPALIVE_CHAT_TABS,
  )
  if (!sameStringList(kept, desired)) {
    setKept(desired)
  }

  return useMemo(() => new Set(kept), [kept])
}

/** Per-tab activity indicator — avoids ContentPane re-render on any stream flag. */
const TabActivityDot = memo(function TabActivityDot({
  sessionId,
  isBrowser,
}: {
  sessionId: SessionId
  isBrowser: boolean
}) {
  const show = useAppStore(
    (s) =>
      !!s.streamingSessions[sessionId] ||
      (isBrowser && s.browserOwnerSessionId === sessionId),
  )
  if (!show) return null
  return (
    <span
      className="ml-0.5 inline-block h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-accent"
      aria-hidden
    />
  )
})

export const ContentPane = ({
  paneIndex,
  keepAliveTools: _keepAliveTools,
  isEastmost = paneIndex === 0,
  onOpenCommandPalette,
  onOpenSearch,
}: ContentPaneProps) => {
  void _keepAliveTools
  const pane = useAppStore(
    (s) => s.contentLayout.panes[paneIndex] ?? EMPTY_PANE,
  )
  const split = useAppStore((s) => s.contentLayout.mode === "split")
  const focusedPane = useAppStore((s) => s.contentLayout.focusedPane)
  const splitEligible = useAppStore(isSplitEligible)
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
  const stampTabGroup = useAppStore((s) => s.stampTabGroup)
  const removeTabsFromGroup = useAppStore((s) => s.removeTabsFromGroup)
  const sidebarCollapsed = useAppStore((s) => s.sidebarCollapsed)
  const { sessions } = useSessions()
  const dragUi = useTabDragUi()
  const [openTabModal, setOpenTabModal] = useState(false)
  const [toolContextError, setToolContextError] = useState<string | null>(null)

  const [selectedTabIds, setSelectedTabIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  )
  const anchorTabIdRef = useRef<string | null>(null)

  const clearSelection = useCallback(() => {
    setSelectedTabIds(new Set())
    anchorTabIdRef.current = null
  }, [])

  const groupIdCounter = useRef(0)

  const handlePickGroupColor = useCallback(
    (color: string) => {
      const ids = Array.from(selectedTabIds)
      if (ids.length < 2) return
      groupIdCounter.current += 1
      const groupId = `g${Date.now()}_${groupIdCounter.current}`
      stampTabGroup(paneIndex, ids, groupId, color)
      clearSelection()
    },
    [selectedTabIds, paneIndex, stampTabGroup, clearSelection],
  )

  const { tabsScrollRef, handleTabsWheel } = useTabStripScroll()

  const keepAliveChats = useChatKeepAliveTabs(pane.tabs, pane.activeTabId)

  const sessionsById = useMemo(
    () => new Map(sessions.map((s) => [s.id, s])),
    [sessions],
  )

  const hasMultipleSessions = useMemo(() => {
    const ids = new Set(pane.tabs.map((t) => t.sessionId))
    return ids.size > 1
  }, [pane.tabs])

  const activeTab = useMemo(
    () => pane.tabs.find((t) => t.id === pane.activeTabId) ?? null,
    [pane.tabs, pane.activeTabId],
  )

  const contextSession: SessionId | null = useMemo(() => {
    if (activeTab) return activeTab.sessionId
    return activeSessionId
  }, [activeTab, activeSessionId])

  const toolContextSession = useMemo(() => {
    if (activeTab?.kind !== "tool" && activeTab?.kind !== "file") return null
    return sessionsById.get(activeTab.sessionId) ?? null
  }, [activeTab, sessionsById])

  useEffect(() => {
    setToolContextError(null)
  }, [pane.activeTabId])

  const cwd = useMemo(
    () => (contextSession ? sessionsById.get(contextSession)?.cwd : undefined),
    [sessionsById, contextSession],
  )

  const prQuery = useQuery({
    queryKey: ["git-pr-status", cwd ?? ""],
    queryFn: () => gitPrStatus(cwd!),
    enabled: false,
    staleTime: 60_000,
  })
  const hasBranchPr = !!prQuery.data?.pr
  const hasPlanReady = useAppStore((s) =>
    sessionHasPlanReady(contextSession, s),
  )
  const catalog = useMemo(
    () => visibleRightPanelTabs({ hasBranchPr, hasPlanReady }),
    [hasBranchPr, hasPlanReady],
  )
  const stripLabels = useMemo(
    () => fullStripCatalog(hasPlanReady),
    [hasPlanReady],
  )
  const paneFocused = focusedPane === paneIndex

  const handleUngroup = useCallback(
    (tabId: string) => {
      removeTabsFromGroup(paneIndex, [tabId])
    },
    [paneIndex, removeTabsFromGroup],
  )

  const { menuPosition, contextMenuItems, onTabContextMenu, closeMenu } =
    useContentPaneContextMenu({
      paneIndex,
      paneTabs: pane.tabs,
      paneGroups: pane.groups ?? {},
      splitEligible,
      openTabToSide,
      closeTabInPane,
      closeOtherTabsInPane,
      closeTabsToRightInPane,
      onRemoveFromGroup: handleUngroup,
    })

  useEffect(() => {
    const id = pane.activeTabId
    if (!id) return
    const el = tabsScrollRef.current?.querySelector<HTMLElement>(
      `[data-tab-id="${CSS.escape(id)}"]`,
    )
    el?.scrollIntoView({ block: "nearest", inline: "nearest" })
  }, [pane.activeTabId, pane.tabs.length, tabsScrollRef])

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
    [tabsScrollRef],
  )

  const fileDraftsBySession = useAppStore((s) => s.fileDraftsBySession)

  const siblingFromPane =
    dragUi?.dragging && dragUi.fromPane !== paneIndex ? dragUi.fromPane : null
  const siblingPaneTabs = useAppStore((s) =>
    siblingFromPane != null
      ? (s.contentLayout.panes[siblingFromPane]?.tabs ?? null)
      : null,
  )

  const dragTabId = dragUi?.dragging ? dragUi.tabId : null
  const sourceTabs = useMemo(() => {
    if (!dragUi?.dragging) return pane.tabs
    if (dragUi.fromPane === paneIndex) return pane.tabs
    return siblingPaneTabs ?? pane.tabs
  }, [dragUi, pane.tabs, paneIndex, siblingPaneTabs])

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

  const showGroupBar = selectedTabIds.size >= 2

  return (
    <div
      className={cn(
        "relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-chrome",
        paneFocused ? "z-[1]" : "z-0",
      )}
      data-content-pane={paneIndex}
      onMouseDown={() => setFocusedPane(paneIndex)}
    >
      <TabStrip
        aria-label={paneIndex === 0 ? "Left pane tabs" : "Right pane tabs"}
        data-slot="glass-titleband"
        className="glass-titleband h-[var(--titlebar-height)] min-w-0 items-center gap-1.5 px-2.5"
        onDoubleClick={() => void toggleZoomWindow()}
        onKeyDown={handleTabsKeyDown}
      >
        {paneIndex === 0 && sidebarCollapsed ? (
          <TitleBarLeading
            showWindowControls
            showSidebarReopen
            onOpenCommandPalette={onOpenCommandPalette}
            onOpenSearch={onOpenSearch}
            className="self-center"
          />
        ) : null}
        <div
          ref={tabsScrollRef}
          role="presentation"
          data-content-tab-strip={paneIndex}
          onWheel={handleTabsWheel}
          className={cn(
            "flex min-w-0 shrink items-center gap-1.5 overflow-x-auto",
            "[scrollbar-width:none] [&::-webkit-scrollbar]:hidden",
          )}
        >
          {displayTabs.map((t, index) => {
            const def =
              t.kind === "tool"
                ? stripLabels.find((c) => c.id === t.tool)
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
            const label = tabLabel(t, sessionsById, stripLabels)
            const FileGlyph =
              t.kind === "file" ? fileIconForPath(t.path) : null

            const dirty =
              t.kind === "file"
                ? !!fileDraftsBySession[sessionScopeKey(t.sessionId)]?.[t.path]
                : false

            const titleText =
              t.kind === "file"
                ? t.path
                : hasMultipleSessions && t.kind === "tool"
                  ? `${label} — ${sessionLabel(sessionsById.get(t.sessionId) ?? { title: t.sessionId.slice(0, 8) } as SessionMeta)}`
                  : label

            const prev = displayTabs[index - 1]
            const showDivider =
              !!prev &&
              (prev.sessionId !== t.sessionId ||
                prev.groupId !== t.groupId ||
                (prev.kind === "chat" && t.kind !== "chat") ||
                (prev.kind !== "chat" && t.kind === "chat"))

            const dotColor = hasMultipleSessions
              ? sessionColor(t.sessionId)
              : undefined

            const isBrowser = t.kind === "tool" && t.tool === "browser"

            const groupColor =
              t.groupId != null && pane.groups?.[t.groupId] != null
                ? pane.groups[t.groupId]!.color
                : undefined

            const isRangeSelected = selectedTabIds.has(t.id)

            const handleTabClick = (e: ReactMouseEvent<HTMLElement>) => {
              if (e.shiftKey) {
                e.preventDefault()
                const anchor = anchorTabIdRef.current
                if (!anchor) {
                  anchorTabIdRef.current = t.id
                  setSelectedTabIds(new Set([t.id]))
                } else {
                  const anchorIdx = displayTabs.findIndex(
                    (dt) => dt.id === anchor,
                  )
                  if (anchorIdx < 0) {
                    anchorTabIdRef.current = t.id
                    setSelectedTabIds(new Set([t.id]))
                  } else {
                    const lo = Math.min(anchorIdx, index)
                    const hi = Math.max(anchorIdx, index)
                    setSelectedTabIds(
                      new Set(
                        displayTabs.slice(lo, hi + 1).map((dt) => dt.id),
                      ),
                    )
                  }
                }
              } else {
                clearSelection()
                activateTabInPane(paneIndex, t.id)
              }
            }

            return (
              <Fragment key={t.id}>
                {showDivider ? (
                  <span
                    aria-hidden
                    className="h-4 w-px shrink-0 bg-stroke-3"
                  />
                ) : null}
                <Tab
                  selected={t.id === pane.activeTabId}
                  icon={
                    t.kind === "chat" ? (
                      <MessageSquare aria-hidden />
                    ) : t.kind === "file" && FileGlyph ? (
                      <FileGlyph aria-hidden />
                    ) : def?.icon ? (
                      <def.icon aria-hidden />
                    ) : undefined
                  }
                  className={cn(
                    "max-w-[180px] shrink-0",
                    "transition-[opacity,transform,colors] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                    isDragged && "opacity-40",
                    t.id === pane.activeTabId &&
                      !paneFocused &&
                      "bg-fill-3 text-ink-secondary",
                  )}
                  title={titleText}
                  tabId={t.id}
                  tabIndex={t.id === pane.activeTabId ? 0 : -1}
                  onSelect={() => activateTabInPane(paneIndex, t.id)}
                  onClick={handleTabClick}
                  onClose={() => closeTabInPane(paneIndex, t.id)}
                  onContextMenu={(e) => onTabContextMenu(e, t.id)}
                  closeLabel={`Close ${label}`}
                  draggable
                  dropEdge={dropEdge}
                  onPointerDown={(e) => {
                    if (e.shiftKey) return
                    startContentTabPointerDrag(e, paneIndex, t.id)
                  }}
                  groupColor={groupColor}
                  badge={
                    <TabActivityDot
                      sessionId={t.sessionId}
                      isBrowser={isBrowser}
                    />
                  }
                  sessionColor={dotColor}
                  rangeSelected={isRangeSelected}
                >
                  {dirty ? "● " : ""}
                  {label}
                </Tab>
              </Fragment>
            )
          })}
        </div>
        <TitleBarDragRegion className="self-stretch" />
        <div className="flex shrink-0 items-center gap-0.5 self-center">
          {showGroupBar ? (
            <GroupSwatchBar onPickColor={handlePickGroupColor} />
          ) : null}
          <OpenTabModal
            open={openTabModal}
            onOpenChange={setOpenTabModal}
            trigger={
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="Open tab"
                title={openTabModal ? undefined : "Open tab"}
                className={cn(
                  "h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink",
                  "opacity-50 hover:opacity-80",
                )}
              />
            }
            paneIndex={paneIndex}
            sessionId={contextSession}
            tabs={catalog}
            onOpenChat={openChatInPane}
            onOpenTool={(p, sid, tool) =>
              openToolInPane(p, sid, tool as RightPanelTab)
            }
          />
          {split ? (
            <Tooltip label="Close pane">
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="Close pane"
                title="Close pane"
                onClick={(e: ReactMouseEvent<HTMLButtonElement>) => {
                  e.stopPropagation()
                  closePane(paneIndex)
                }}
                className="h-6 w-6 text-ink-muted opacity-50 hover:bg-fill-4 hover:text-ink hover:opacity-80"
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </Tooltip>
          ) : null}
          {isEastmost ? <TitleBarTrailing showChatActions /> : null}
        </div>
      </TabStrip>

      <div className="relative min-h-0 flex-1">
        {pane.tabs.map((t) => {
          const isActive = t.id === pane.activeTabId
          if (t.kind === "chat") {
            // Mount active chat + LRU keep-alive set only (not every visited tab).
            if (!shouldMountChatTab(t.id, isActive, keepAliveChats)) return null
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
          if (t.kind === "file") {
            const dirty =
              !!fileDraftsBySession[sessionScopeKey(t.sessionId)]?.[t.path]
            // Mount only when active or dirty so Monaco/query cost stays bounded.
            if (!shouldMountFileTab(isActive, dirty)) return null
            return (
              <div
                key={t.id}
                className={cn(
                  "absolute inset-0 flex flex-col",
                  isActive ? "flex" : "hidden",
                )}
              >
                <PanelErrorBoundary label="File">
                  <FileDocumentTab
                    path={t.path}
                    session={sessionsById.get(t.sessionId)}
                    active={isActive}
                  />
                </PanelErrorBoundary>
              </div>
            )
          }
          return (
            <ToolTabBody
              key={t.id}
              tool={t.tool}
              session={sessionsById.get(t.sessionId)}
              active={isActive}
              // Open tool tabs stay mounted (CSS-hidden) so tab switches are solid.
              keepAlive
            />
          )
        })}
        {pane.tabs.length === 0 ? (
          <div className="flex h-full items-center justify-center px-2.5 text-sm text-ink-muted">
            Open a chat or tool tab with +
          </div>
        ) : null}
      </div>

      {toolContextSession ? (
        <div
          className="shrink-0 border-t border-stroke-3 px-2.5 py-0.5"
          data-pane-context-bar
        >
          {toolContextError ? (
            <div className="mb-0.5">
              <ErrorBanner
                message={toolContextError}
                onDismiss={() => setToolContextError(null)}
              />
            </div>
          ) : null}
          <ContextBar
            cwd={toolContextSession.cwd}
            projectCwd={
              toolContextSession.base_cwd || toolContextSession.cwd
            }
            sessionId={toolContextSession.id}
            disabled={!paneFocused}
            onError={setToolContextError}
            quiet
          />
        </div>
      ) : null}

      <ContextMenu
        position={menuPosition}
        items={contextMenuItems}
        onClose={closeMenu}
      />
    </div>
  )
}
