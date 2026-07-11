import {
  useMemo,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  Archive,
  ArrowUpRight,
  Bot,
  Brain,
  ChevronDown,
  Moon,
  RotateCw,
  Search,
  Settings,
  SlidersHorizontal,
  SquarePen,
  Sun,
  X,
} from "lucide-react"
import { Button, IconButton, ScrollArea, Skeleton, Spinner } from "../atoms"
import {
  ContextMenu,
  EmptyState,
  ErrorBanner,
  RepoSectionHeader,
  SessionListItem,
  SidebarActionRow,
  type ContextMenuItem,
} from "../molecules"
import { useQueryClient } from "@tanstack/react-query"
import { SESSIONS_KEY, useSessions } from "../../hooks/useSessions"
import { useWorkspaceStatuses } from "../../hooks/useWorkspaceStatuses"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import { isSessionNotFoundError } from "../../lib/sessions"
import type { SessionMeta } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { persistUiState, useAppStore } from "../../stores/appStore"
import { SIDEBAR_DEFAULT_WIDTH } from "../../stores/layoutConstants"

const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/i.test(navigator.platform)

type RepoGroup = {
  cwd: string
  label: string
  sessions: SessionMeta[]
  latestMs: number
}

const groupByRepo = (sessions: SessionMeta[]): RepoGroup[] => {
  const groups = new Map<string, RepoGroup>()
  for (const session of sessions) {
    const key = session.cwd || "~"
    let group = groups.get(key)
    if (!group) {
      group = { cwd: key, label: basename(key), sessions: [], latestMs: 0 }
      groups.set(key, group)
    }
    group.sessions.push(session)
    group.latestMs = Math.max(group.latestMs, session.updated_at_ms)
  }
  const sorted = [...groups.values()].sort((a, b) => b.latestMs - a.latestMs)
  for (const group of sorted) {
    group.sessions.sort((a, b) => b.updated_at_ms - a.updated_at_ms)
  }
  return sorted
}

type SessionSidebarProps = {
  onOpenSearch: () => void
}

export const SessionSidebar = ({ onOpenSearch }: SessionSidebarProps) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const unreadBySession = useAppStore((s) => s.unreadBySession)
  const theme = useAppStore((s) => s.theme)
  const toggleTheme = useAppStore((s) => s.toggleTheme)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const sidebarWidth = useAppStore((s) => s.sidebarWidth)
  const setSidebarWidth = useAppStore((s) => s.setSidebarWidth)
  const setSidebarCollapsed = useAppStore((s) => s.setSidebarCollapsed)
  const viewport = useAppStore((s) => s.viewport)
  const narrow = viewport !== "wide"
  const pinnedSessionIds = useAppStore((s) => s.pinnedSessionIds)
  const archivedSessionIds = useAppStore((s) => s.archivedSessionIds)
  const toggleSessionPinned = useAppStore((s) => s.toggleSessionPinned)
  const setSessionArchived = useAppStore((s) => s.setSessionArchived)
  const [selectError, setSelectError] = useState<string | null>(null)
  const [selectErrorId, setSelectErrorId] = useState<string | null>(null)
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({})
  const [collapsedRepos, setCollapsedRepos] = useState<Record<string, boolean>>({})
  const [archivedCollapsed, setArchivedCollapsed] = useState(true)
  const [dragging, setDragging] = useState(false)
  const [repoMenu, setRepoMenu] = useState<{
    cwd: string
    position: { x: number; y: number }
  } | null>(null)
  const {
    sessions: allSessions,
    isLoading,
    error,
    newAgent,
    renameSession,
    deleteSession,
    isCreating,
  } = useSessions()
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  // Subagent child sessions render inside their parent's feed — the sidebar
  // lists only top-level agents.
  const sessions = useMemo(
    () => allSessions.filter((s) => !s.parent_id),
    [allSessions],
  )

  const handleCreate = async (cwd?: string) => {
    await newAgent(cwd)
    if (narrow) setSidebarCollapsed(true)
  }

  /** Everywhere `resume_session` can fail with "not found" (the session's
   * row/id no longer exists engine-side — e.g. a delete that raced with a
   * resume, or a persisted id from a previous run): drop it from the
   * react-query list cache, clear the persisted activeSessionId if it
   * matches, toast, and — critically — do NOT surface a Retry banner, since
   * retrying a resume for an id that will never exist again is meaningless. */
  const healNotFoundSession = (id: string) => {
    queryClient.setQueryData<SessionMeta[]>(SESSIONS_KEY, (prev) =>
      prev ? prev.filter((s) => s.id !== id) : prev,
    )
    void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
    const state = useAppStore.getState()
    if (state.activeSessionId === id) {
      setActiveSessionId(null)
      void persistUiState({ activeSessionId: null })
    }
    pushToast("Session no longer exists", "error")
  }

  const handleSelect = async (id: string) => {
    // Clear only this row's stale error banner/state — leave other rows alone.
    if (selectErrorId === id) {
      setSelectError(null)
      setSelectErrorId(null)
    }
    try {
      await resumeSession(id)
      setRowErrors((prev) => {
        if (!(id in prev)) return prev
        const next = { ...prev }
        delete next[id]
        return next
      })
      setActiveSessionId(id)
      setRoute("chat")
      if (narrow) setSidebarCollapsed(true)
    } catch (err) {
      console.error("resume_session failed", id, err)
      const message = toInvokeError(err)
      const notFound = isSessionNotFoundError(message)
      if (notFound) {
        setRowErrors((prev) => {
          if (!(id in prev)) return prev
          const next = { ...prev }
          delete next[id]
          return next
        })
        healNotFoundSession(id)
        return
      }
      setSelectError(message)
      setSelectErrorId(id)
      setRowErrors((prev) => ({ ...prev, [id]: message }))
    }
  }

  const handleDismissSelectError = () => {
    setSelectError(null)
    setSelectErrorId(null)
  }

  const handleRetrySelect = () => {
    if (selectErrorId) void handleSelect(selectErrorId)
  }

  const pinnedIdSet = useMemo(() => new Set(pinnedSessionIds), [pinnedSessionIds])
  const archivedIdSet = useMemo(
    () => new Set(archivedSessionIds),
    [archivedSessionIds],
  )

  const pinnedSessions = useMemo(
    () => sessions.filter((s) => pinnedIdSet.has(s.id)),
    [sessions, pinnedIdSet],
  )

  const archivedSessions = useMemo(
    () => sessions.filter((s) => archivedIdSet.has(s.id)),
    [sessions, archivedIdSet],
  )

  const groupableSessions = useMemo(
    () => sessions.filter((s) => !pinnedIdSet.has(s.id) && !archivedIdSet.has(s.id)),
    [sessions, pinnedIdSet, archivedIdSet],
  )

  const repoGroups = useMemo(() => groupByRepo(groupableSessions), [groupableSessions])

  const sessionIds = useMemo(() => sessions.map((s) => s.id), [sessions])
  const workspaceStatuses = useWorkspaceStatuses(sessionIds)

  const toggleRepo = (cwd: string) =>
    setCollapsedRepos((prev) => ({ ...prev, [cwd]: !prev[cwd] }))

  const handleRepoContextMenu = (
    e: ReactMouseEvent<HTMLDivElement>,
    cwd: string,
  ) => {
    e.preventDefault()
    setRepoMenu({ cwd, position: { x: e.clientX, y: e.clientY } })
  }

  const repoMenuItems: ContextMenuItem[] = repoMenu
    ? [
        {
          type: "item",
          label: "New Agent here",
          icon: SquarePen,
          onSelect: () => void handleCreate(repoMenu.cwd),
        },
        {
          type: "item",
          label: collapsedRepos[repoMenu.cwd] ? "Expand" : "Collapse",
          onSelect: () => toggleRepo(repoMenu.cwd),
        },
      ]
    : []

  const handleSashDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault()
    setDragging(true)
    const startX = e.clientX
    const startWidth = sidebarWidth

    const onMove = (ev: globalThis.PointerEvent) => {
      // Sidebar is on the left — dragging right grows it.
      setSidebarWidth(startWidth + (ev.clientX - startX), false)
    }
    const onUp = (ev: globalThis.PointerEvent) => {
      setSidebarWidth(startWidth + (ev.clientX - startX), true)
      setDragging(false)
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }

  const handleSashDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    setSidebarWidth(SIDEBAR_DEFAULT_WIDTH, true)
  }

  const expanded = !collapsed
  return (
    <>
      {narrow && expanded ? (
        <div
          className="absolute inset-0 z-20 bg-black/30 animate-backdrop-in"
          aria-hidden
          onClick={() => setSidebarCollapsed(true)}
        />
      ) : null}
      <aside
        style={!collapsed && !narrow ? { width: sidebarWidth } : undefined}
        className={cn(
          "relative flex h-full shrink-0 flex-col overflow-hidden bg-surface",
          !dragging &&
            "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
          collapsed
            ? "w-0 border-r-0 opacity-0 pointer-events-none"
            : "border-r border-stroke-3 opacity-100",
          // Mobile (narrow/tight): full-width overlay anchored to the app's
          // left edge instead of a side-by-side column — same open/close
          // state, now floating above the chat with a shadow (mirrors
          // RightPanel.tsx's narrow handling).
          narrow && expanded
            ? "absolute inset-y-0 left-0 z-30 w-full shadow-popover"
            : null,
        )}
        aria-hidden={collapsed}
        aria-label="Sessions sidebar"
      >
        {expanded && !narrow ? (
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize sessions sidebar"
            aria-valuenow={sidebarWidth}
            tabIndex={0}
            onPointerDown={handleSashDown}
            onDoubleClick={handleSashDoubleClick}
            className={cn(
              "absolute -right-[5px] inset-y-0 z-10 w-2.5 cursor-col-resize",
              "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
              "after:transition-colors after:duration-[var(--duration-fast)] hover:after:bg-stroke-2",
              dragging && "after:bg-stroke-1",
            )}
          />
        ) : null}

      {narrow && expanded ? (
        // Full-width overlay only — wide mode keeps the existing header
        // (backdrop click is enough at side-by-side width; discoverability
        // requires an explicit close control once the sidebar fills the
        // chat area).
        <div className="flex h-[var(--header-height)] shrink-0 items-center justify-between border-b border-stroke-3 px-3">
          <span className="text-sm text-ink-secondary">Sessions</span>
          <IconButton label="Close sidebar" onClick={() => setSidebarCollapsed(true)}>
            <X className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </div>
      ) : null}

      <div className="flex flex-col gap-px px-2 pt-2 pb-3">
        <SidebarActionRow
          icon={SquarePen}
          label="New Agent"
          kbd={isMac ? "⌘N" : "Ctrl+N"}
          onClick={() => void handleCreate()}
        />
        <SidebarActionRow
          icon={Search}
          label="Search"
          kbd={isMac ? "⌘K" : "Ctrl+K"}
          onClick={() => {
            onOpenSearch()
            if (narrow) setSidebarCollapsed(true)
          }}
        />
        <SidebarActionRow
          icon={Bot}
          label="Automations"
          trailingIcon={ArrowUpRight}
          onClick={() => {
            setRoute("automations")
            if (narrow) setSidebarCollapsed(true)
          }}
        />
        <SidebarActionRow
          icon={Brain}
          label="Memory"
          trailingIcon={ArrowUpRight}
          onClick={() => {
            setRoute("memory")
            if (narrow) setSidebarCollapsed(true)
          }}
        />
        <SidebarActionRow
          icon={SlidersHorizontal}
          label="Customize"
          onClick={() => {
            setRoute("customize")
            if (narrow) setSidebarCollapsed(true)
          }}
        />
      </div>

      <div className="group/label flex items-center gap-1 px-2 pb-1">
        <span className="min-w-0 flex-1 truncate px-2 text-sm text-ink-muted">
          Repositories
        </span>
        <IconButton
          label="Search agents"
          className={cn(
            "h-6 w-6 opacity-0 transition-opacity duration-[var(--duration-fast)]",
            "group-hover/label:opacity-100",
          )}
          onClick={onOpenSearch}
        >
          <Search className="h-3 w-3" aria-hidden />
        </IconButton>
      </div>

      {error ? (
        <div className="px-2 pb-2">
          <ErrorBanner message={error} />
        </div>
      ) : null}

      <ScrollArea className="flex-1 px-2 pb-2">
        {isLoading ? (
          <div className="flex flex-col gap-1 p-1">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-7 w-full rounded-sm" />
            ))}
          </div>
        ) : sessions.length === 0 ? (
          <EmptyState
            title="No agents yet"
            description="Create an agent to start working on tasks."
            actionLabel="New Agent"
            onAction={() => void handleCreate()}
          />
        ) : (
          <div className="flex flex-col gap-2">
            {pinnedSessions.length > 0 ? (
              <section className="flex flex-col gap-px">
                <div className="flex h-6 w-full items-center gap-1.5 px-1.5 text-xs text-ink-secondary">
                  <span className="min-w-0 flex-1 truncate">Pinned</span>
                </div>
                {pinnedSessions.map((session) => (
                  <SessionListItem
                    key={session.id}
                    session={session}
                    isActive={session.id === activeSessionId}
                    isRunning={!!streamingSessions[session.id]}
                    errorMessage={rowErrors[session.id]}
                    unread={unreadBySession[session.id]}
                    workspaceStatus={workspaceStatuses[session.id]}
                    pinned
                    onSelect={(id) => void handleSelect(id)}
                    onRename={renameSession}
                    onDelete={deleteSession}
                    onNewAgentInRepo={(cwd) => void handleCreate(cwd)}
                    onTogglePin={toggleSessionPinned}
                    onSetArchived={setSessionArchived}
                  />
                ))}
              </section>
            ) : null}

            {repoGroups.map((group) => (
              <section key={group.cwd} className="flex flex-col gap-px">
                <div
                  onContextMenu={(e) => handleRepoContextMenu(e, group.cwd)}
                >
                  <RepoSectionHeader
                    label={group.label}
                    collapsed={!!collapsedRepos[group.cwd]}
                    onToggle={() => toggleRepo(group.cwd)}
                    onNewSession={() => void handleCreate(group.cwd)}
                  />
                </div>
                {!collapsedRepos[group.cwd]
                  ? group.sessions.map((session) => (
                      <SessionListItem
                        key={session.id}
                        session={session}
                        isActive={session.id === activeSessionId}
                        isRunning={!!streamingSessions[session.id]}
                        errorMessage={rowErrors[session.id]}
                        unread={unreadBySession[session.id]}
                        workspaceStatus={workspaceStatuses[session.id]}
                        onSelect={(id) => void handleSelect(id)}
                        onRename={renameSession}
                        onDelete={deleteSession}
                        onNewAgentInRepo={(cwd) => void handleCreate(cwd)}
                        onTogglePin={toggleSessionPinned}
                        onSetArchived={setSessionArchived}
                      />
                    ))
                  : null}
              </section>
            ))}

            {archivedSessions.length > 0 ? (
              <section className="flex flex-col gap-px">
                <div
                  role="button"
                  tabIndex={0}
                  aria-expanded={!archivedCollapsed}
                  onClick={() => setArchivedCollapsed((prev) => !prev)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") setArchivedCollapsed((prev) => !prev)
                  }}
                  className={cn(
                    "group flex h-6 w-full cursor-default items-center gap-1.5 rounded-sm px-1.5",
                    "text-xs text-ink-secondary transition-colors hover:bg-fill-4 hover:text-ink",
                  )}
                >
                  <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
                    <ChevronDown
                      className={cn(
                        "h-3.5 w-3.5 text-icon-2 opacity-70 transition-[opacity,transform] group-hover:opacity-100",
                        archivedCollapsed && "-rotate-90",
                      )}
                      aria-hidden
                    />
                  </span>
                  <Archive className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
                  <span className="min-w-0 flex-1 truncate">
                    Archived ({archivedSessions.length})
                  </span>
                </div>
                {!archivedCollapsed
                  ? archivedSessions.map((session) => (
                      <SessionListItem
                        key={session.id}
                        session={session}
                        isActive={session.id === activeSessionId}
                        isRunning={!!streamingSessions[session.id]}
                        errorMessage={rowErrors[session.id]}
                        unread={unreadBySession[session.id]}
                        workspaceStatus={workspaceStatuses[session.id]}
                        archived
                        onSelect={(id) => void handleSelect(id)}
                        onRename={renameSession}
                        onDelete={deleteSession}
                        onNewAgentInRepo={(cwd) => void handleCreate(cwd)}
                        onTogglePin={toggleSessionPinned}
                        onSetArchived={setSessionArchived}
                      />
                    ))
                  : null}
              </section>
            ) : null}
          </div>
        )}
      </ScrollArea>

      {selectError ? (
        <div
          role="alert"
          className={cn(
            "flex items-start gap-2 border-t border-stroke-3 bg-danger-subtle px-3 py-2",
          )}
        >
          <p className="flex-1 text-xs text-danger">{selectError}</p>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-1.5 text-danger"
            onClick={handleRetrySelect}
          >
            <RotateCw className="h-3 w-3" aria-hidden />
            Retry
          </Button>
          <IconButton label="Dismiss error" onClick={handleDismissSelectError}>
            <X className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </div>
      ) : null}

      <div className="flex items-center justify-end gap-0.5 border-t border-stroke-3 px-2 py-1.5">
        <IconButton
          quiet
          label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          onClick={toggleTheme}
        >
          {theme === "dark" ? (
            <Sun className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <Moon className="h-3.5 w-3.5" aria-hidden />
          )}
        </IconButton>
        <IconButton
          quiet
          label="Settings"
          onClick={() => {
            setRoute("settings")
            if (narrow) setSidebarCollapsed(true)
          }}
        >
          <Settings className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      {isCreating ? (
        <div className="flex items-center gap-2 border-t border-stroke-3 px-4 py-2 text-xs text-ink-muted">
          <Spinner size="sm" />
          Creating…
        </div>
      ) : null}

      <ContextMenu
        position={repoMenu?.position ?? null}
        items={repoMenuItems}
        onClose={() => setRepoMenu(null)}
      />
      </aside>
    </>
  )
}
