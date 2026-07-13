import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { ChevronsLeft } from "lucide-react"
import { IconButton } from "../atoms"
import { ContextMenu, type ContextMenuItem } from "../molecules"
import { gitStatusSinceBaseline } from "../../lib/tauri"
import { useAppStore, sessionScopeKey, type RightPanelTab } from "../../stores/appStore"
import { RIGHT_PANEL_DEFAULT_WIDTH } from "../../stores/layoutConstants"
import { useSessions } from "../../hooks/useSessions"
import { useIsGitRepo } from "../../hooks/useIsGitRepo"
import { cn } from "../../lib/utils"
import { BrowserTab } from "./BrowserTab"
import { TerminalTab } from "./TerminalTab"
import { PlanTab } from "./right-panel/PlanTab"
import { ChangesTab } from "./right-panel/ChangesTab"
import { RightPanelTabBar } from "./right-panel/RightPanelTabBar"
import { TABS } from "./right-panel/tabs"

export { TABS } from "./right-panel/tabs"

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
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
  const sessionKey = sessionScopeKey(activeSessionId)
  const openTabsBySession = useAppStore((s) => s.openTabsBySession)
  const openTab = useAppStore((s) => s.openTab)
  const closeTab = useAppStore((s) => s.closeTab)
  const collapsed = useAppStore((s) => s.rightPanelCollapsed)
  const setCollapsed = useAppStore((s) => s.setRightPanelCollapsed)
  const [addMenuPos, setAddMenuPos] = useState<{ x: number; y: number } | null>(
    null,
  )

  // Tabs are on-demand ("Open Tabs") — only render the tabs the
  // session has actually opened (via a trigger or the "+" menu below), never
  // the full static TABS list.
  const openIds = openTabsBySession[sessionKey] ?? []
  const openTabDefs = useMemo(
    () => TABS.filter((t) => openIds.includes(t.id)),
    [openIds],
  )
  const closableTabDefs = useMemo(
    () => TABS.filter((t) => !openIds.includes(t.id)),
    [openIds],
  )

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

  const addMenuItems: ContextMenuItem[] = closableTabDefs.map((t) => ({
    type: "item",
    label: t.label,
    icon: t.icon,
    onSelect: () => {
      openTab(sessionKey, t.id)
      setTab(t.id)
    },
  }))

  // auto-reveal: the moment a plan awaits approval for the
  // active session, surface it — open the panel and switch to Plan — instead
  // of leaving it to a background tab the user might not be looking at.
  const awaitingApprovalForActive =
    !!activeSessionId &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === activeSessionId
  const prevAwaitingRef = useRef(false)
  useEffect(() => {
    if (awaitingApprovalForActive && !prevAwaitingRef.current) {
      setRightPanelOpen(true)
      setTab("plan")
    }
    prevAwaitingRef.current = awaitingApprovalForActive
  }, [awaitingApprovalForActive, setRightPanelOpen, setTab])

  // Gate the tab's changes-count badge on the cwd being a git repo — see
  // ChangesTab's own `isRepo` gating for the full rationale.
  const isRepoForBadge = useIsGitRepo(active?.cwd).data

  const changesSummary = useQuery({
    queryKey: ["git-status", active?.cwd ?? "", active?.id ?? null],
    queryFn: () => gitStatusSinceBaseline(active!.id),
    enabled: !!active?.cwd && !!active?.id && open && isRepoForBadge !== false,
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
    const startX = e.clientX
    const startWidth = width

    const onMove = (ev: globalThis.PointerEvent) => {
      // Panel is on the right — dragging left grows it.
      setWidth(startWidth + (startX - ev.clientX), false)
    }
    const onUp = (ev: globalThis.PointerEvent) => {
      setWidth(startWidth + (startX - ev.clientX), true)
      setDragging(false)
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

  // Collapsed variant: a slim icon strip, wide layout only (narrow/tight
  // already collapse via the full-panel overlay mechanics). Re-expanding
  // keeps every tab's underlying state intact (terminal PTY / browser
  // webview never unmount here) — this is purely presentational.
  //
  // Self-heal: an open+collapsed panel with zero open tabs has no icons and
  // no "+" (the add control lives on the expanded tab bar). Expand so the
  // user can open a tab instead of staring at an empty « strip.
  useEffect(() => {
    if (open && collapsed && !narrow && openIds.length === 0) {
      setCollapsed(false)
    }
  }, [open, collapsed, narrow, openIds.length, setCollapsed])

  if (open && collapsed && !narrow) {
    return (
      <aside
        style={{ width: 40 }}
        className="flex h-full shrink-0 flex-col items-center gap-1 border-l border-stroke-3 bg-bg py-1.5"
        aria-label="Details panel (collapsed)"
      >
        <IconButton label="Expand panel" onClick={() => setCollapsed(false)}>
          <ChevronsLeft className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        {openTabDefs.map((t) => {
          const Icon = t.icon
          return (
            <IconButton
              key={t.id}
              label={t.label}
              onClick={() => {
                setTab(t.id)
                setCollapsed(false)
              }}
              className={cn(tab === t.id && "bg-fill-2 text-ink")}
            >
              {Icon ? <Icon className="h-3.5 w-3.5" aria-hidden /> : null}
            </IconButton>
          )
        })}
      </aside>
    )
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
          onCollapse={() => setCollapsed(true)}
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
          ) : null}
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
