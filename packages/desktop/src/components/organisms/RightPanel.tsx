import {
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { Globe, Terminal as TerminalIcon, X } from "lucide-react"
import { IconButton } from "../atoms"
import {
  gitIsRepo,
  gitStatusSinceBaseline,
} from "../../lib/tauri"
import { useAppStore, type RightPanelTab } from "../../stores/appStore"
import { RIGHT_PANEL_DEFAULT_WIDTH } from "../../stores/layoutConstants"
import { useSessions } from "../../hooks/useSessions"
import { cn } from "../../lib/utils"
import { BrowserTab } from "./BrowserTab"
import { TerminalTab } from "./TerminalTab"
import { PlanTab } from "./right-panel/PlanTab"
import { ChangesTab } from "./right-panel/ChangesTab"

const TABS: Array<{
  id: RightPanelTab
  label: string
  icon?: typeof TerminalIcon
}> = [
  { id: "plan", label: "Plan" },
  { id: "changes", label: "Changes" },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "browser", label: "Browser", icon: Globe },
]

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
  const isRepoForBadge = useQuery({
    queryKey: ["git-is-repo", active?.cwd ?? ""],
    queryFn: () => gitIsRepo(active!.cwd),
    enabled: !!active?.cwd,
    staleTime: 15_000,
  }).data

  const changesCount = useQuery({
    queryKey: ["git-status", active?.cwd ?? "", active?.id ?? null],
    queryFn: () => gitStatusSinceBaseline(active!.id),
    enabled: !!active?.cwd && !!active?.id && open && isRepoForBadge !== false,
    refetchInterval: 10_000,
  }).data?.length

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

        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-3 border-b border-stroke-3 px-1">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              aria-selected={tab === t.id}
              role="tab"
              className={cn(
                "flex items-center gap-1.5 rounded-[4px] px-1.5 py-[2px] text-sm",
                "tracking-[var(--tracking-caption)]",
                "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                tab === t.id
                  ? "bg-fill-2 text-ink"
                  : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
              )}
            >
              {t.icon ? <t.icon className="h-3.5 w-3.5" aria-hidden /> : null}
              {t.label}
              {t.id === "changes" && changesCount ? (
                <span className="text-ink-faint [font-variant-numeric:tabular-nums]">
                  {changesCount}
                </span>
              ) : null}
            </button>
          ))}
          {narrow ? (
            // Full-width overlay only — wide mode has no header close button
            // (AppHeader's ⌘J toggle covers it there) and must stay
            // byte-identical; at narrow the panel fills the chat area so a
            // backdrop click alone is undiscoverable.
            <IconButton
              label="Close panel"
              onClick={() => setRightPanelOpen(false)}
              className="ml-auto"
            >
              <X className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          ) : null}
        </div>

        {tab === "plan" ? (
          <PlanTab active={active} />
        ) : tab === "changes" ? (
          <ChangesTab active={active} />
        ) : null}
        <div
          className={cn(
            "min-h-0 flex-1 flex-col",
            tab === "terminal" ? "flex" : "hidden",
          )}
        >
          <TerminalTab active={open && tab === "terminal"} />
        </div>
        <div
          className={cn(
            "min-h-0 flex-1 flex-col",
            tab === "browser" ? "flex" : "hidden",
          )}
        >
          <BrowserTab active={open && tab === "browser"} />
        </div>
      </aside>
    </>
  )
}
