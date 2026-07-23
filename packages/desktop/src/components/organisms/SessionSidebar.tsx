import { Button } from "@/components/ui/button"
import {
  SidebarContent,
  SidebarFooter as SbFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  Bot,
  Brain,
  FolderPlus,
  PanelLeft,
  Search,
  SquarePen,
} from "lucide-react"
import {
  ArchivedSectionHeader,
  EmptyState,
  ErrorBanner,
  RepoSectionHeader,
  SessionListItem,
  SidebarActionRow,
  SidebarFooter,
  SidebarProjectFilter,
  SidebarResumeError,
  SidebarSkeleton,
  reposHeaderIconClass,
} from "../molecules"
import { TitleBarMenus } from "../molecules/TitleBarMenus"
import { TrafficLights } from "../molecules/WindowControls"
import { BugReportDialog } from "../molecules/BugReportDialog"
import { TitleBarDragRegion } from "./TitleBarChrome"
import { useTitleBarActions } from "../../hooks/useTitleBarActions"
import { detectWindowHost } from "../../lib/windowChrome"
import { useQueryClient } from "@tanstack/react-query"
import { SESSIONS_KEY, useSessions } from "../../hooks/useSessions"
import { useWorkspaceStatuses } from "../../hooks/useWorkspaceStatuses"
import { useGitStatuses } from "../../hooks/useGitStatuses"
import { useIndexedRepos } from "../../hooks/useIndexedRepos"
import { useSessionSidebarGroups } from "../../hooks/useSessionSidebarGroups"
import {
  resumeSession,
  toInvokeError,
} from "../../lib/tauri"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import { isSessionNotFoundError } from "../../lib/sessions"
import { nestSessionsByParent } from "../../lib/sessionGrouping"
import { isPristineSession, type SessionMeta } from "../../lib/types"
import { cn } from "../../lib/utils"
import { persistUiState, useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
} from "../../stores/layoutConstants"

type SessionSidebarProps = {
  onOpenSearch: () => void
}

export const SessionSidebar = ({ onOpenSearch }: SessionSidebarProps) => {
  const host = detectWindowHost()
  const isMacHost = host === "macos"
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const theme = useAppStore((s) => s.theme)
  const toggleTheme = useAppStore((s) => s.toggleTheme)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const sidebarWidth = useAppStore((s) => s.sidebarWidth)
  const setSidebarWidth = useAppStore((s) => s.setSidebarWidth)
  const setSidebarCollapsed = useAppStore((s) => s.setSidebarCollapsed)
  const viewport = useAppStore((s) => s.viewport)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const narrow = viewport !== "wide"
  const [bugOpen, setBugOpen] = useState(false)
  const openBugReport = useCallback(() => setBugOpen(true), [])
  const closeBugReport = useCallback(() => setBugOpen(false), [])
  // newAgent is declared below via useSessions — wire menus after that hook.
  const pinnedSessionIds = useAppStore((s) => s.pinnedSessionIds)
  const archivedSessionIds = useAppStore((s) => s.archivedSessionIds)
  const sidebarProjectSort = useAppStore((s) => s.sidebarProjectSort)
  const sidebarProjectVisibility = useAppStore((s) => s.sidebarProjectVisibility)
  const setSidebarProjectSort = useAppStore((s) => s.setSidebarProjectSort)
  const setSidebarProjectVisibility = useAppStore(
    (s) => s.setSidebarProjectVisibility,
  )
  const toggleSessionPinned = useAppStore((s) => s.toggleSessionPinned)
  const setSessionArchived = useAppStore((s) => s.setSessionArchived)
  const pushToast = useAppStore((s) => s.pushToast)
  const handleSetArchived = useCallback(
    (id: string, archived: boolean) => {
      setSessionArchived(id, archived)
      if (archived) {
        pushToast("Session archived", "success", {
          label: "Undo",
          onAction: () => setSessionArchived(id, false),
        })
      }
    },
    [setSessionArchived, pushToast],
  )
  const [selectError, setSelectError] = useState<string | null>(null)
  const [selectErrorId, setSelectErrorId] = useState<string | null>(null)
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({})
  const [archivedCollapsed, setArchivedCollapsed] = useState(true)
  const [collapsedRepos, setCollapsedRepos] = useState<Record<string, boolean>>(
    {},
  )
  const [dragging, setDragging] = useState(false)
  const {
    sessions: allSessions,
    isLoading,
    isFetching,
    error,
    newAgent,
    renameSession,
    deleteSession,
    isCreating,
  } = useSessions()
  const { handlers: titleBarHandlers } = useTitleBarActions({
    newAgent,
    onOpenSearch,
    onOpenBugReport: openBugReport,
  })
  const queryClient = useQueryClient()
  // Roots fill time buckets / pins; children (`parent_id`) nest under roots.
  const sessions = useMemo(
    () => allSessions.filter((s) => !s.parent_id),
    [allSessions],
  )
  const childrenByParent = useMemo(() => {
    const map = new Map<string, SessionMeta[]>()
    for (const node of nestSessionsByParent(sessions, allSessions)) {
      if (node.children.length > 0) map.set(node.session.id, node.children)
    }
    return map
  }, [sessions, allSessions])

  const activeSession = useMemo(
    () =>
      allSessions.find((s) => s.id === activeSessionId) ??
      sessions.find((s) => s.id === activeSessionId) ??
      null,
    [allSessions, sessions, activeSessionId],
  )

  // Drop persisted active/pin ids that are no longer in the sessions list
  // (engine restarted, deleted elsewhere) so status polls never target ghosts.
  // Skip while the list is loading/refetching — a just-created session can be
  // selected before invalidateQueries lands, and clearing would wipe the
  // content pane to the empty "Open a chat or tool tab with +" state.
  useEffect(() => {
    if (isLoading || isFetching) return
    const known = new Set(allSessions.map((s) => s.id))
    if (activeSessionId && !known.has(activeSessionId)) {
      setActiveSessionId(null)
      void persistUiState({ activeSessionId: null })
    }
    const pinnedGone = pinnedSessionIds.some((id) => !known.has(id))
    if (pinnedGone) {
      const next = pinnedSessionIds.filter((id) => known.has(id))
      useAppStore.getState().setPinnedSessionIds(next)
      void persistUiState({ pinnedSessionIds: next })
    }
  }, [
    isLoading,
    isFetching,
    allSessions,
    activeSessionId,
    pinnedSessionIds,
    setActiveSessionId,
  ])

  const { pinnedSessions, archivedSessions, repoGroups } =
    useSessionSidebarGroups(sessions, pinnedSessionIds, archivedSessionIds, {
      sort: sidebarProjectSort,
      visibility: sidebarProjectVisibility,
      activeSession,
    })

  const handleCreate = useCallback(
    async (cwd?: string) => {
      // Collapse overlay immediately so the click feels responsive even when
      // create_session waits on isolation / engine work. Do not await —
      // mutation onSuccess selects the new session; awaiting here left the
      // sidebar button dead until create + post-select git/`gh` finished.
      if (narrow) setSidebarCollapsed(true)
      else setSidebarCollapsed(false)
      void newAgent(cwd).catch((err: unknown) => {
        pushToast(toInvokeError(err), "error")
      })
    },
    [newAgent, narrow, setSidebarCollapsed, pushToast],
  )

  /** Everywhere `resume_session` can fail with "not found" (the session's
   * row/id no longer exists engine-side — e.g. a delete that raced with a
   * resume, or a persisted id from a previous run): drop it from the
   * react-query list cache, clear the persisted activeSessionId if it
   * matches, toast, and — critically — do NOT surface a Retry banner, since
   * retrying a resume for an id that will never exist again is meaningless. */
  const healNotFoundSession = useCallback(
    (id: string) => {
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
    },
    [queryClient, setActiveSessionId, pushToast],
  )

  const handleSelect = useCallback(
    async (id: string) => {
      // Empty New Agent drafts open as a single-tab composition —
      // prune sibling Changes/Files tabs + collapse split.
      const sessions =
        queryClient.getQueryData<SessionMeta[]>(SESSIONS_KEY) ?? []
      const meta = sessions.find((s) => s.id === id)
      const pristineDraft = !!meta && isPristineSession(meta)
      const activateOpts = pristineDraft
        ? ({ panel: "closed" } as const)
        : undefined

      if (id === activeSessionId) {
        // Re-open the chat tab if the user closed every tab but stayed
        // "active" on this session (empty "+ " placeholder).
        setActiveSessionId(id, activateOpts)
        setRoute("chat")
        if (narrow) setSidebarCollapsed(true)
        return
      }

      // Clear only this row's stale error banner/state — leave other rows alone.
      setSelectErrorId((prevId) => {
        if (prevId === id) {
          setSelectError(null)
          return null
        }
        return prevId
      })

      // Paint the target chat immediately — resume is warm-up, not a gate.
      setActiveSessionId(id, activateOpts)
      setRoute("chat")
      if (narrow) setSidebarCollapsed(true)

      try {
        await resumeSession(id)
        setRowErrors((prev) => {
          if (!(id in prev)) return prev
          const next = { ...prev }
          delete next[id]
          return next
        })
      } catch (err) {
        const message = toInvokeError(err)
        log.error("session", "resume_session failed", { sessionId: id, error: message })
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
    },
    [
      activeSessionId,
      healNotFoundSession,
      narrow,
      queryClient,
      setActiveSessionId,
      setRoute,
      setSidebarCollapsed,
    ],
  )

  const handleSelectRow = useCallback(
    (id: string) => {
      void handleSelect(id)
    },
    [handleSelect],
  )

  const handleNewAgentInRepo = useCallback(
    (cwd: string) => {
      void handleCreate(cwd)
    },
    [handleCreate],
  )

  const handleDismissSelectError = () => {
    setSelectError(null)
    setSelectErrorId(null)
  }

  const handleRetrySelect = () => {
    if (selectErrorId) void handleSelect(selectErrorId)
  }

  // Poll only sessions that both exist in the list cache and are visible
  // (plus active + pinned when present). Never poll a stale persisted
  // activeSessionId / pin that is gone engine-side — that path used to
  // hammer `workspace_status` with "session not found" for ~6s each.
  const knownSessionIds = useMemo(
    () => new Set(sessions.map((s) => s.id)),
    [sessions],
  )
  const statusPollIds = useMemo(() => {
    const ids = new Set<string>()
    const addIfKnown = (id: string | null | undefined) => {
      if (id && knownSessionIds.has(id)) ids.add(id)
    }
    addIfKnown(activeSessionId)
    for (const id of pinnedSessionIds) addIfKnown(id)
    for (const group of repoGroups) {
      if (collapsedRepos[group.cwd]) continue
      for (const s of group.sessions) {
        ids.add(s.id)
        for (const child of childrenByParent.get(s.id) ?? []) ids.add(child.id)
      }
    }
    for (const s of pinnedSessions) {
      for (const child of childrenByParent.get(s.id) ?? []) ids.add(child.id)
    }
    if (!archivedCollapsed) {
      for (const s of archivedSessions) ids.add(s.id)
    }
    return ids
  }, [
    knownSessionIds,
    activeSessionId,
    pinnedSessionIds,
    repoGroups,
    collapsedRepos,
    pinnedSessions,
    childrenByParent,
    archivedCollapsed,
    archivedSessions,
  ])
  const sessionIdsForPoll = useMemo(
    () => [...statusPollIds],
    [statusPollIds],
  )
  const sessionCwdsForPoll = useMemo(
    () =>
      allSessions
        // Skip pristine drafts — no baseline yet, so git_status_since_baseline
        // would return full-repo dirty and paint a lying DiffStat on "New Agent".
        .filter((s) => statusPollIds.has(s.id) && !isPristineSession(s))
        .map((s) => ({ id: s.id, cwd: s.cwd })),
    [allSessions, statusPollIds],
  )
  const statusPollOptions = useMemo(
    () => ({
      pollingEnabled: !collapsed,
      pollIds: statusPollIds,
    }),
    [collapsed, statusPollIds],
  )
  const workspaceStatuses = useWorkspaceStatuses(sessionIdsForPoll, statusPollOptions)
  const gitStatuses = useGitStatuses(sessionCwdsForPoll, statusPollOptions)

  const repoCwds = useMemo(() => repoGroups.map((g) => g.cwd), [repoGroups])
  const indexedRepos = useIndexedRepos(repoCwds)

  const toggleRepo = useCallback((cwd: string) => {
    setCollapsedRepos((prev) => ({ ...prev, [cwd]: !prev[cwd] }))
  }, [])

  const handleSashDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    setDragging(true)
    const startX = e.clientX
    const startWidth = sidebarWidth
    try {
      e.currentTarget.setPointerCapture(e.pointerId)
    } catch {
      // Capture is best-effort — window listeners still drive the drag.
    }

    const onMove = (ev: globalThis.PointerEvent) => {
      // Sidebar is on the left — dragging right grows it.
      setSidebarWidth(startWidth + (ev.clientX - startX), false)
    }
    const onUp = (ev: globalThis.PointerEvent) => {
      setSidebarWidth(startWidth + (ev.clientX - startX), true)
      setDragging(false)
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
      window.removeEventListener("pointercancel", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
    window.addEventListener("pointercancel", onUp)
  }

  const handleSashDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    setSidebarWidth(SIDEBAR_DEFAULT_WIDTH, true)
  }

  const handleSashKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 32 : 8
    let next: number | null = null
    if (e.key === "ArrowLeft") next = sidebarWidth - step
    if (e.key === "ArrowRight") next = sidebarWidth + step
    if (e.key === "Home") next = SIDEBAR_MIN_WIDTH
    if (e.key === "End") next = SIDEBAR_MAX_WIDTH
    if (e.key === "Escape") {
      e.currentTarget.blur()
      return
    }
    if (next === null) return
    e.preventDefault()
    setSidebarWidth(next, true)
  }

  const expanded = !collapsed

  /** Root row + nested children (`parent_id`). */
  const renderSessionTree = (
    session: SessionMeta,
    opts: { pinned?: boolean; archived?: boolean } = {},
  ) => {
    const children = childrenByParent.get(session.id) ?? []
    return (
      <div key={session.id} className="flex flex-col gap-px">
        <SidebarMenuItem>
          <SessionListItem
            session={session}
            isActive={session.id === activeSessionId}
            errorMessage={rowErrors[session.id]}
            workspaceStatus={workspaceStatuses[session.id]}
            gitStatus={gitStatuses[session.id]}
            pinned={opts.pinned}
            archived={opts.archived}
            onSelect={handleSelectRow}
            onRename={renameSession}
            onDelete={deleteSession}
            onNewAgentInRepo={handleNewAgentInRepo}
            onTogglePin={opts.archived ? undefined : toggleSessionPinned}
            onSetArchived={handleSetArchived}
          />
        </SidebarMenuItem>
        {children.map((child) => (
          <SidebarMenuItem key={child.id}>
            <SessionListItem
              session={child}
              isActive={child.id === activeSessionId}
              errorMessage={rowErrors[child.id]}
              workspaceStatus={workspaceStatuses[child.id]}
              gitStatus={gitStatuses[child.id]}
              nestDepth={1}
              roleLabel={child.role ?? null}
              archived={opts.archived}
              onSelect={handleSelectRow}
              onRename={renameSession}
              onDelete={deleteSession}
              onNewAgentInRepo={handleNewAgentInRepo}
              onSetArchived={handleSetArchived}
            />
          </SidebarMenuItem>
        ))}
      </div>
    )
  }

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
          // overflow-visible so the sash (positioned past the right edge) can
          // receive pointer events; inner shell clips list/nav content.
          "relative flex h-full shrink-0 flex-col overflow-visible bg-sidebar",
          !dragging &&
            "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)] motion-reduce:transition-none",
          collapsed
            ? "w-0 border-r-0 opacity-0 pointer-events-none"
            : "border-r border-sidebar-border opacity-100",
          // Mobile (narrow/tight): full-width overlay anchored to the app's
          // left edge instead of a side-by-side column — same open/close
          // state, now floating above the chat with a shadow.
          narrow && expanded
            ? "absolute inset-y-0 left-0 z-30 w-full overflow-hidden shadow-popover"
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
            aria-valuemin={SIDEBAR_MIN_WIDTH}
            aria-valuemax={SIDEBAR_MAX_WIDTH}
            aria-valuenow={sidebarWidth}
            tabIndex={0}
            onPointerDown={handleSashDown}
            onDoubleClick={handleSashDoubleClick}
            onKeyDown={handleSashKeyDown}
            className={cn(
              // z-30 above content pane so the hit target isn't covered.
              "sash-line-transition absolute -right-[5px] inset-y-0 z-30 w-2.5 cursor-col-resize",
              "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
              // Quiet sash: white-alpha hover only — never accent (Feel: Quiet chrome).
              "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_12%,transparent)]",
              dragging && "after:bg-stroke-1",
            )}
          />
        ) : null}

        <div className="flex h-full min-h-0 w-full flex-col overflow-hidden">
          <SidebarHeader className="gap-0 p-0">
            <div className="flex flex-col gap-1 pb-3">
              {/* Top chrome aligns with ContentPane TabStrip (`--titlebar-height`).
               * Dedicated TitleBarDragRegion so the undecorated window can move;
               * traffic lights / collapse stay no-drag via button CSS. */}
              <div className="flex h-[var(--titlebar-height)] items-center gap-1 border-b border-stroke-3 px-2">
                <div className="flex shrink-0 items-center gap-0.5">
                  {isMacHost ? (
                    <div className="flex h-full items-center pl-1.5 pr-0.5">
                      <TrafficLights />
                    </div>
                  ) : null}
                  {!isMacHost ? (
                    <TitleBarMenus
                      handlers={titleBarHandlers}
                      isBootstrapped={isBootstrapped}
                      canSearch
                      canCommandPalette={false}
                    />
                  ) : null}
                </div>
                <TitleBarDragRegion />
                <div className="flex shrink-0 items-center gap-px">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    aria-label="Toggle left sidebar"
                    title="Toggle left sidebar"
                    onClick={() => setSidebarCollapsed(true)}
                    className={cn(
                      "h-6 w-6 text-icon-2 hover:bg-fill-4 hover:text-icon-1",
                      "opacity-50 hover:opacity-80",
                    )}
                  >
                    <PanelLeft className="size-3.5" strokeWidth={1.5} aria-hidden />
                  </Button>
                </div>
              </div>

              <nav className="flex flex-col gap-px pb-1" aria-label="Agent navigation">
                <SidebarMenu className="gap-px">
                  <SidebarMenuItem>
                    <SidebarActionRow
                      icon={SquarePen}
                      label="New Agent"
                      kbd={isMacHost ? "⌘N" : "Ctrl+N"}
                      onClick={() => void handleCreate()}
                      disabled={isCreating}
                      loading={isCreating}
                    />
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarActionRow
                      icon={Search}
                      label="Search"
                      kbd={isMacHost ? "⌘K" : "Ctrl+K"}
                      onClick={() => {
                        onOpenSearch()
                        if (narrow) setSidebarCollapsed(true)
                      }}
                    />
                  </SidebarMenuItem>
                  {AUTOMATIONS_UI_ENABLED ? (
                    <SidebarMenuItem>
                      <SidebarActionRow
                        icon={Bot}
                        label="Automations"
                        onClick={() => {
                          setRoute("automations")
                          if (narrow) setSidebarCollapsed(true)
                        }}
                      />
                    </SidebarMenuItem>
                  ) : null}
                  <SidebarMenuItem>
                    <SidebarActionRow
                      icon={Brain}
                      label="Memory"
                      onClick={() => {
                        setRoute("memory")
                        if (narrow) setSidebarCollapsed(true)
                      }}
                    />
                  </SidebarMenuItem>
                </SidebarMenu>
              </nav>

              <div className="flex items-center gap-0.5 px-2.5 pb-0.5">
                <span className="min-w-0 flex-1 truncate text-xs tracking-[var(--tracking-caption)] text-ink-muted">
                  Repositories
                </span>
                <SidebarProjectFilter
                  sort={sidebarProjectSort}
                  visibility={sidebarProjectVisibility}
                  onSortChange={setSidebarProjectSort}
                  onVisibilityChange={setSidebarProjectVisibility}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label="New project"
                  title="Open folder as project"
                  disabled={!isBootstrapped || isCreating}
                  onClick={() => titleBarHandlers.openFolder()}
                  className={reposHeaderIconClass}
                >
                  <FolderPlus className="size-3.5" strokeWidth={1.5} aria-hidden />
                </Button>
              </div>
            </div>
          </SidebarHeader>

          {error ? (
            <div className="px-2.5 pb-2">
              <ErrorBanner message={error} />
            </div>
          ) : null}

          {/* Gutters live on rows (DESIGN: no px-2.5 on SidebarContent — avoids double indent). */}
          <SidebarContent className="pb-2">
            {isLoading ? (
              <SidebarSkeleton />
            ) : sessions.length === 0 ? (
              <EmptyState
                title="No agents yet"
                description="Create an agent to start working on tasks."
                actionLabel="New Agent"
                onAction={() => void handleCreate()}
                actionDisabled={isCreating}
              />
            ) : (
              <div className="flex flex-col gap-px">
                {pinnedSessions.length > 0 ? (
                  <SidebarGroup className="gap-0 p-0">
                    <SidebarGroupLabel className="flex h-6 items-center px-2.5 pl-8 text-sm font-normal text-ink-muted">
                      Pinned
                    </SidebarGroupLabel>
                    <SidebarMenu className="gap-px">
                      {pinnedSessions.map((session) =>
                        renderSessionTree(session, { pinned: true }),
                      )}
                    </SidebarMenu>
                  </SidebarGroup>
                ) : null}

                {repoGroups.map((group) => (
                  <SidebarGroup key={group.cwd} className="gap-0 p-0">
                    <div className="px-1.5">
                      <RepoSectionHeader
                        label={group.label}
                        collapsed={!!collapsedRepos[group.cwd]}
                        onToggle={() => toggleRepo(group.cwd)}
                        onNewSession={() => void handleCreate(group.cwd)}
                        indexed={!!indexedRepos[group.cwd]}
                        isCreating={isCreating}
                      />
                    </div>
                    {!collapsedRepos[group.cwd] ? (
                      <SidebarMenu className="gap-px">
                        {group.sessions.map((session) =>
                          renderSessionTree(session),
                        )}
                      </SidebarMenu>
                    ) : null}
                  </SidebarGroup>
                ))}

                {pinnedSessions.length === 0 &&
                repoGroups.length === 0 &&
                sidebarProjectVisibility === "active" ? (
                  <EmptyState
                    title="No active projects"
                    description="Nothing updated in the last 14 days. Switch to All projects to see everything."
                    actionLabel="Show all projects"
                    onAction={() => setSidebarProjectVisibility("all")}
                  />
                ) : null}

                {archivedSessions.length > 0 ? (
                  <SidebarGroup className="p-0 gap-0">
                    <ArchivedSectionHeader
                      count={archivedSessions.length}
                      collapsed={archivedCollapsed}
                      onToggle={() => setArchivedCollapsed((prev) => !prev)}
                    />
                    {!archivedCollapsed ? (
                      <SidebarMenu className="gap-px">
                        {archivedSessions.map((session) =>
                          renderSessionTree(session, { archived: true }),
                        )}
                      </SidebarMenu>
                    ) : null}
                  </SidebarGroup>
                ) : null}
              </div>
            )}
          </SidebarContent>

          {selectError ? (
            <SidebarResumeError
              message={selectError}
              onRetry={handleRetrySelect}
              onDismiss={handleDismissSelectError}
            />
          ) : null}

          <SbFooter className="gap-0 p-0">
            <SidebarFooter
              theme={theme}
              onToggleTheme={toggleTheme}
              onOpenSettings={() => {
                setRoute("settings")
                if (narrow) setSidebarCollapsed(true)
              }}
              isCreating={isCreating}
            />
          </SbFooter>
        </div>
      </aside>
      {!isMacHost ? (
        <BugReportDialog open={bugOpen} onClose={closeBugReport} />
      ) : null}
    </>
  )
}

