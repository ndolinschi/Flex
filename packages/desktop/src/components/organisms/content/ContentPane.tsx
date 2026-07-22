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
} from "react"
import { useQuery } from "@tanstack/react-query"
import { MessageSquare, X } from "lucide-react"
import { Tab, TabStrip, Tooltip } from "../../atoms"
import { ContextMenu, OpenTabModal } from "../../molecules"
import { useSessions } from "../../../hooks/useSessions"
import {
  startContentTabPointerDrag,
  useTabDragUi,
} from "../../../hooks/useContentTabPointerDnD"
import { useTabStripScrollFade } from "../../../hooks/useTabStripScrollFade"
import { useContentPaneContextMenu } from "../../../hooks/useContentPaneContextMenu"
import { previewTabsForPane } from "../../../lib/tabDnD"
import { gitPrStatus } from "../../../lib/tauri"
import { sessionLabel, type SessionId, type SessionMeta } from "../../../lib/types"
import {
  useAppStore,
  type ContentTab,
  type RightPanelTab,
} from "../../../stores/appStore"
import { isSplitEligible } from "../../../stores/slices/contentLayoutSlice"
import { emptyPane } from "../../../stores/contentLayoutModel"
import { visibleRightPanelTabs } from "../right-panel/tabs"
import { ChatSessionBody } from "./ChatSessionBody"
import { ToolTabBody } from "./ToolTabBody"
import { cn } from "../../../lib/utils"
import { sessionColor, GROUP_PALETTE } from "../../../lib/sessionColor"

type ContentPaneProps = {
  paneIndex: 0 | 1
  /** Tool tabs that must stay mounted (browser/terminal/files) across panes. */
  keepAliveTools: Set<string>
}

/**
 * Labels/icons for open strip tabs — always include PR so an open PR tab keeps
 * its chrome after the branch PR goes away.
 *
 * Call at render time (not module load): `ContentPane` is imported via `App`
 * before `registerBuiltinUiPlugins()` runs in `main.tsx`, so a module-level
 * snapshot would omit plugin tabs (Artifacts, Database, …) and their icons
 * would never appear in the TabStrip.
 */
const fullStripCatalog = () => visibleRightPanelTabs({ hasBranchPr: true })

/** Stable fallback when a pane index is missing (must keep object identity). */
const EMPTY_PANE = emptyPane()

// ─── Group color swatch bar ───────────────────────────────────────────────────

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
  return catalog.find((c) => c.id === tab.tool)?.label ?? tab.tool
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

export const ContentPane = ({ paneIndex, keepAliveTools }: ContentPaneProps) => {
  // Narrow selectors — avoid re-rendering this pane when only the sibling
  // pane's tabs change (structural sharing in activate/reorder/close).
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
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const browserOwnerSessionId = useAppStore((s) => s.browserOwnerSessionId)
  const stampTabGroup = useAppStore((s) => s.stampTabGroup)
  const removeTabsFromGroup = useAppStore((s) => s.removeTabsFromGroup)
  const { sessions } = useSessions()
  const dragUi = useTabDragUi()
  const [openTabModal, setOpenTabModal] = useState(false)

  // ─── SHIFT range-select state ──────────────────────────────────────────────
  // Ephemeral — not persisted; clears when the pane unmounts or on plain click.
  const [selectedTabIds, setSelectedTabIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  )
  const anchorTabIdRef = useRef<string | null>(null)

  const clearSelection = useCallback(() => {
    setSelectedTabIds(new Set())
    anchorTabIdRef.current = null
  }, [])

  // ─── Group picker ──────────────────────────────────────────────────────────
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

  const { tabsScrollRef, scrollMask, handleTabsWheel } = useTabStripScrollFade(
    pane.tabs.length,
  )

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

  const contextSession: SessionId | null = useMemo(() => {
    const active = pane.tabs.find((t) => t.id === pane.activeTabId)
    if (active) return active.sessionId
    return activeSessionId
  }, [pane.tabs, pane.activeTabId, activeSessionId])

  const cwd = useMemo(
    () => (contextSession ? sessionsById.get(contextSession)?.cwd : undefined),
    [sessionsById, contextSession],
  )

  // Never fetch PR status when opening the + menu — `gh pr view` can stall
  // the first click. Reuse cache populated by BranchPicker / tab lifecycle.
  const prQuery = useQuery({
    queryKey: ["git-pr-status", cwd ?? ""],
    queryFn: () => gitPrStatus(cwd!),
    enabled: false,
    staleTime: 60_000,
  })
  const hasBranchPr = !!prQuery.data?.pr
  const catalog = useMemo(
    () => visibleRightPanelTabs({ hasBranchPr }),
    [hasBranchPr],
  )
  // Full catalog (PR always included) for strip labels/icons of already-open tabs.
  // Empty deps: plugin registry is fixed after boot in main.tsx.
  const stripLabels = useMemo(() => fullStripCatalog(), [])
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

  // Keep the active tab visible when it changes or tabs are added.
  useEffect(() => {
    const id = pane.activeTabId
    if (!id) return
    const el = tabsScrollRef.current?.querySelector<HTMLElement>(
      `[data-tab-id="${CSS.escape(id)}"]`,
    )
    el?.scrollIntoView({ block: "nearest", inline: "nearest" })
  }, [pane.activeTabId, pane.tabs.length, tabsScrollRef])

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
    [tabsScrollRef],
  )

  // Reactive sibling pane tabs for drag preview (avoids getState() inside useMemo).
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
        // Continuous chrome surface (`--color-chrome` via bg-bg); no card stack.
        "relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-bg",
        paneFocused ? "z-[1]" : "z-0",
      )}
      data-content-pane={paneIndex}
      onMouseDown={() => setFocusedPane(paneIndex)}
    >
      <TabStrip
        aria-label={paneIndex === 0 ? "Left pane tabs" : "Right pane tabs"}
        // gap/height owned by TabStrip (1.5 / 30px); do not override.
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

            // Session ownership cue: when multiple sessions share a pane,
            // suffix tool-tab titles with the owning session label.
            const titleText =
              hasMultipleSessions && t.kind === "tool"
                ? `${label} — ${sessionLabel(sessionsById.get(t.sessionId) ?? { title: t.sessionId.slice(0, 8) } as SessionMeta)}`
                : label

            // Divider between the last chat tab and first tool tab.
            const prev = displayTabs[index - 1]
            const showDivider = prev?.kind === "chat" && t.kind === "tool"

            // Session affinity dot: shown when multiple sessions share the pane.
            const dotColor = hasMultipleSessions
              ? sessionColor(t.sessionId)
              : undefined

            // Activity indicators: streaming chat tabs + browser-owner tool tabs.
            const isBrowser = t.kind === "tool" && t.tool === "browser"
            const isStreaming = streamingSessions[t.sessionId] ?? false
            const isBrowserOwner =
              isBrowser && browserOwnerSessionId === t.sessionId
            const showActivity = isStreaming || isBrowserOwner

            // Group underbar: read color from the pane's groups map.
            const groupColor =
              t.groupId != null && pane.groups?.[t.groupId] != null
                ? pane.groups[t.groupId]!.color
                : undefined

            // SHIFT range-selection highlight.
            const isRangeSelected = selectedTabIds.has(t.id)

            // SHIFT+click opens range selection; plain click activates + clears.
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
                  onClick={handleTabClick}
                  onClose={() => closeTabInPane(paneIndex, t.id)}
                  onContextMenu={(e) => onTabContextMenu(e, t.id)}
                  closeLabel={`Close ${label}`}
                  draggable
                  dropEdge={dropEdge}
                  onPointerDown={(e) => {
                    // Skip DnD initiation when SHIFT is held (range-select gesture).
                    if (e.shiftKey) return
                    startContentTabPointerDrag(e, paneIndex, t.id)
                  }}
                  groupColor={groupColor}
                  activityDot={showActivity}
                  sessionColor={dotColor}
                  rangeSelected={isRangeSelected}
                >
                  {label}
                </Tab>
              </Fragment>
            )
          })}
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
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
                title="Open tab"
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
              session={sessionsById.get(t.sessionId)}
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
        onClose={closeMenu}
      />
    </div>
  )
}
