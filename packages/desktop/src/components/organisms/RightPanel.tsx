import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { ContextMenu, type ContextMenuItem } from "../molecules"
import { gitPrStatus, gitStatusSinceBaseline } from "../../lib/tauri"
import {
  useAppStore,
  sessionScopeKey,
  type RightPanelTab,
  type TerminalMeta,
} from "../../stores/appStore"
import { RIGHT_PANEL_DEFAULT_WIDTH } from "../../stores/layoutConstants"
import { useSessions } from "../../hooks/useSessions"
import { useIsGitRepo } from "../../hooks/useIsGitRepo"
import { basename, cn } from "../../lib/utils"
import { BrowserTab } from "./BrowserTab"
import { TerminalTab } from "./TerminalTab"
import { PlanTab } from "./right-panel/PlanTab"
import { ChangesTab } from "./right-panel/ChangesTab"
import { FilesTab } from "./right-panel/FilesTab"
import { MemoryTab } from "./right-panel/MemoryTab"
import { PrTab } from "./right-panel/PrTab"
import { RightPanelMiniTabs } from "./right-panel/RightPanelMiniTabs"
import { RightPanelTabBar } from "./right-panel/RightPanelTabBar"
import { visibleRightPanelTabs } from "./right-panel/tabs"
import { findPluginTab } from "../../plugins/registry"

export { TABS, visibleRightPanelTabs } from "./right-panel/tabs"

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_TERMINALS: TerminalMeta[] = []

export const RightPanel = () => {
  const rightPanelOpen = useAppStore((s) => s.rightPanelOpen)
  const route = useAppStore((s) => s.route)
  const viewport = useAppStore((s) => s.viewport)
  const narrow = viewport !== "wide"
  // Overlay routes (settings/automations/memory/customize) render as absolute
  // panes over ChatPage (see App.tsx) — the right panel belongs to chat only,
  // so it must hide there. Compute an *effective* open instead of unmounting:
  // terminals/webview must survive the route swap (PTYs / the native browser
  // webview are expensive to recreate), so this reuses the exact same
  // width-0/hidden mechanics as the user manually closing the panel.
  const open = rightPanelOpen && route === "chat"
  const tab = useAppStore((s) => s.rightPanelTab)
  const setTab = useAppStore((s) => s.setRightPanelTab)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const width = useAppStore((s) => s.rightPanelWidth)
  const setWidth = useAppStore((s) => s.setRightPanelWidth)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const { sessions } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const [dragging, setDragging] = useState(false)
  const setRightPanelDragging = useAppStore((s) => s.setRightPanelDragging)
  const sessionKey = sessionScopeKey(activeSessionId)
  const openTabsBySession = useAppStore((s) => s.openTabsBySession)
  const openTab = useAppStore((s) => s.openTab)
  const closeTab = useAppStore((s) => s.closeTab)
  const collapsed = useAppStore((s) => s.rightPanelCollapsed)
  const setCollapsed = useAppStore((s) => s.setRightPanelCollapsed)
  const terminals = useAppStore(
    (s) => s.terminalsBySession[sessionKey] ?? EMPTY_TERMINALS,
  )
  const terminalCount = terminals.length
  const [addMenuPos, setAddMenuPos] = useState<{ x: number; y: number } | null>(
    null,
  )
  // Cursor-style mini-tabs flyout — wide chat only (hidden on narrow/tight
  // overlays where it would collide with the chat rail).
  const showMiniTabs =
    route === "chat" &&
    !rightPanelOpen &&
    !!activeSessionId &&
    viewport === "wide"

  // Slim "collapsed strip" was a second right-panel control alongside
  // AppHeader's PanelRight — Cursor-style is one toggle. Clear any
  // persisted collapsed bit so bootstrap never resurrects the strip.
  useEffect(() => {
    if (collapsed) setCollapsed(false)
  }, [collapsed, setCollapsed])

  // PR tab is catalog-gated on current-branch PR presence (see lifecycle
  // effect below). Poll even when the panel is closed so an agent-created PR
  // can surface the tab without the user opening Changes first.
  const prStatusQuery = useQuery({
    queryKey: ["git-pr-status", active?.cwd ?? ""],
    queryFn: () => gitPrStatus(active!.cwd),
    enabled: !!active?.cwd && route === "chat",
    refetchInterval: 15_000,
  })
  const hasBranchPr = !!prStatusQuery.data?.pr

  // Tabs are on-demand ("Open Tabs") — only render the tabs the
  // session has actually opened (via a trigger or the "+" menu below), never
  // the full static TABS list. Flag-gated tabs (Memory) are omitted when off;
  // Pull Request only when `hasBranchPr`.
  const openIdsRaw = openTabsBySession[sessionKey] ?? []
  const catalog = useMemo(
    () => visibleRightPanelTabs({ hasBranchPr }),
    [hasBranchPr],
  )
  const openIds = openIdsRaw.filter((id) =>
    catalog.some((t) => t.id === id),
  )
  const openTabDefs = catalog.filter((t) => openIds.includes(t.id))
  const closableTabDefs = catalog.filter((t) => !openIds.includes(t.id))

  // If the currently-selected tab just got closed, fall back to whatever's
  // still open so the header never shows a highlighted tab with no button.
  useEffect(() => {
    if (openIds.length > 0 && !openIds.includes(tab)) {
      setTab(openIds[openIds.length - 1])
    }
  }, [openIds, tab, setTab])

  const handleCloseTab = (id: RightPanelTab) => {
    closeTab(sessionKey, id)
    if (openIds.length <= 1) {
      // Closing the last open tab — nothing left to show, collapse the panel
      // itself rather than leave an empty header floating.
      setRightPanelOpen(false)
      return
    }
    if (tab === id) {
      const remaining = openIds.filter((t) => t !== id)
      setTab(remaining[remaining.length - 1])
    }
  }

  const selectTabAndOpen = (id: RightPanelTab) => {
    openTab(sessionKey, id)
    setTab(id)
    setRightPanelOpen(true)
  }

  const addMenuItems: ContextMenuItem[] = closableTabDefs.map((t) => ({
    type: "item",
    label: t.label,
    icon: t.icon,
    onSelect: () => {
      openTab(sessionKey, t.id)
      setTab(t.id)
    },
  }))

  // Auto-reveal Plan when approval arms for the active session — including
  // switching into a session that already has a pending plan. Also re-open
  // when the panel is closed while approval is still pending AND the plan id
  // just changed (fresh hand-off), so a closed sidebar never swallows a new
  // ExitPlanMode.
  const awaitingApprovalForActive =
    !!activeSessionId &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === activeSessionId
  const awaitingPlanId = pendingPlanApproval?.planId ?? null
  const prevAwaitingRef = useRef<{ active: boolean; planId: string | null }>({
    active: false,
    planId: null,
  })
  useEffect(() => {
    const prev = prevAwaitingRef.current
    const armed =
      awaitingApprovalForActive &&
      (!prev.active || prev.planId !== awaitingPlanId)
    if (armed) {
      setRightPanelOpen(true)
      setTab("plan")
    }
    prevAwaitingRef.current = {
      active: awaitingApprovalForActive,
      planId: awaitingPlanId,
    }
  }, [awaitingApprovalForActive, awaitingPlanId, setRightPanelOpen, setTab])

  // Pull Request tab: open when a PR appears for this branch; remove when gone.
  // Cold boot / session switch with an existing PR registers the tab in the
  // strip but does not force the panel open — only a null→PR transition does
  // (agent just created a PR).
  const prevHadPrRef = useRef<boolean | null>(null)
  useEffect(() => {
    prevHadPrRef.current = null
  }, [sessionKey, active?.cwd])
  useEffect(() => {
    if (!activeSessionId || !active?.cwd) return
    if (prStatusQuery.data === undefined) return
    const has = !!prStatusQuery.data.pr
    const prev = prevHadPrRef.current
    if (prev === null) {
      if (has) openTab(sessionKey, "pr")
      prevHadPrRef.current = has
      return
    }
    if (!prev && has) {
      openTab(sessionKey, "pr")
      setTab("pr")
      setRightPanelOpen(true)
    } else if (prev && !has) {
      closeTab(sessionKey, "pr")
    }
    prevHadPrRef.current = has
  }, [
    activeSessionId,
    active?.cwd,
    prStatusQuery.data,
    sessionKey,
    openTab,
    closeTab,
    setTab,
    setRightPanelOpen,
  ])

  // Gate the tab's changes-count badge on the cwd being a git repo — see
  // ChangesTab's own `isRepo` gating for the full rationale.
  const isRepoForBadge = useIsGitRepo(active?.cwd).data

  const changesSummary = useQuery({
    queryKey: ["git-status", active?.cwd ?? "", active?.id ?? null],
    queryFn: () => gitStatusSinceBaseline(active!.id),
    // Badge for the open TabStrip *and* the closed-panel mini-tabs flyout.
    enabled:
      !!active?.cwd &&
      !!active?.id &&
      (open || showMiniTabs) &&
      isRepoForBadge !== false,
    refetchInterval: 10_000,
  }).data
  // `totalCount`/totals come from the summary (not the capped `files` row
  // list) so this tab badge always reflects every changed file, even for a
  // session with more changes than the server-side row cap.
  const changesCount = changesSummary?.totalCount
  const changesTotals = useMemo(
    () => ({
      added: changesSummary?.totalAdded ?? 0,
      removed: changesSummary?.totalRemoved ?? 0,
    }),
    [changesSummary],
  )

  const handleSashDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault()
    setDragging(true)
    setRightPanelDragging(true)
    const startX = e.clientX
    const startWidth = width

    const onMove = (ev: globalThis.PointerEvent) => {
      // Panel is on the right — dragging left grows it.
      setWidth(startWidth + (startX - ev.clientX), false)
    }
    const onUp = (ev: globalThis.PointerEvent) => {
      setWidth(startWidth + (startX - ev.clientX), true)
      setDragging(false)
      setRightPanelDragging(false)
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }

  const handleSashDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    setWidth(RIGHT_PANEL_DEFAULT_WIDTH, true)
  }

  return (
    <>
      {narrow && open ? (
        <div
          className="absolute inset-0 z-20 bg-black/30 animate-backdrop-in"
          aria-hidden
          onClick={() => setRightPanelOpen(false)}
        />
      ) : null}
      {showMiniTabs ? (
        <RightPanelMiniTabs
          openTabDefs={openTabDefs}
          selectedTab={tab}
          changesTotals={changesTotals}
          terminalCount={terminalCount}
          catalog={catalog}
          projectLabel={basename(active?.base_cwd || active?.cwd || "project")}
          onSelectTab={selectTabAndOpen}
        />
      ) : null}
      <aside
        style={open ? { width } : undefined}
        className={cn(
          "relative flex h-full shrink-0 flex-col overflow-hidden bg-bg",
          !dragging &&
            "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
          open
            ? "border-l border-stroke-3 opacity-100"
            : "w-0 border-l-0 opacity-0 pointer-events-none",
          // Narrow: overlay anchored to the right edge instead of a side-by-side
          // column — same width clamp, now floating above the chat with a shadow.
          narrow && open ? "absolute inset-y-0 right-0 z-30 shadow-popover" : null,
        )}
        aria-hidden={!open}
        aria-label="Details panel"
      >
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize details panel"
          aria-valuenow={width}
          tabIndex={0}
          onPointerDown={handleSashDown}
          onDoubleClick={handleSashDoubleClick}
          className={cn(
            "sash-line-transition absolute inset-y-0 -left-[5px] z-10 w-2.5 cursor-col-resize",
            "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
            // Sash hover = white-alpha focusBorder, never accent (Feel: Quiet chrome).
            "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_15%,transparent)]",
            dragging && "after:bg-stroke-1",
          )}
        />

        <RightPanelTabBar
          openTabDefs={openTabDefs}
          closableTabDefs={closableTabDefs}
          tab={tab}
          narrow={narrow}
          changesCount={changesCount}
          changesTotals={changesTotals}
          onSelectTab={setTab}
          onCloseTab={handleCloseTab}
          onOpenAddMenu={(e) => setAddMenuPos({ x: e.clientX, y: e.clientY })}
          onClosePanel={() => setRightPanelOpen(false)}
        />

        {/* Body under the tab strip — flex-1 + absolute children so Browser /
         * Terminal always get the full remaining panel height (flex-only
         * hosts were measuring short and leaving a black gap under the
         * native webview). */}
        <div className="relative min-h-0 flex-1">
          {tab === "plan" && openIds.includes("plan") ? (
            <div className="absolute inset-0 flex flex-col">
              <PlanTab active={active} />
            </div>
          ) : tab === "changes" && openIds.includes("changes") ? (
            <div className="absolute inset-0 flex flex-col">
              <ChangesTab active={active} />
            </div>
          ) : tab === "pr" && openIds.includes("pr") ? (
            <div className="absolute inset-0 flex flex-col">
              <PrTab active={active} />
            </div>
          ) : tab === "memory" && openIds.includes("memory") ? (
            <div className="absolute inset-0 flex flex-col">
              <MemoryTab />
            </div>
          ) : null}
          {/* Plugin-contributed tabs (e.g. Database) — no per-tab hardcoding. */}
          {(() => {
            const pluginTab = findPluginTab(tab)
            if (!pluginTab || !openIds.includes(tab)) return null
            // Built-ins above already handle plan/changes/pr/memory; plugins own the rest.
            if (
              tab === "plan" ||
              tab === "changes" ||
              tab === "pr" ||
              tab === "memory" ||
              tab === "files" ||
              tab === "terminal" ||
              tab === "browser"
            ) {
              return null
            }
            return (
              <div className="absolute inset-0 flex flex-col">
                {pluginTab.render({
                  active: open && tab === pluginTab.id,
                  session: active,
                })}
              </div>
            )
          })()}
          <div
            className={cn(
              "absolute inset-0 flex flex-col",
              tab === "files" && openIds.includes("files")
                ? "flex"
                : "hidden",
            )}
          >
            <FilesTab active={open && tab === "files"} />
          </div>
          <div
            className={cn(
              "absolute inset-0 flex flex-col",
              tab === "terminal" && openIds.includes("terminal")
                ? "flex"
                : "hidden",
            )}
          >
            <TerminalTab active={open && tab === "terminal"} />
          </div>
          <div
            className={cn(
              "absolute inset-0",
              tab === "browser" && openIds.includes("browser")
                ? "block"
                : "hidden",
            )}
          >
            <BrowserTab active={open && tab === "browser"} />
          </div>
        </div>
      </aside>

      <ContextMenu
        position={addMenuPos}
        items={addMenuItems}
        onClose={() => setAddMenuPos(null)}
      />
    </>
  )
}
