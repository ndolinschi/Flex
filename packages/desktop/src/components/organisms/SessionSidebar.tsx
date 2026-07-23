import { Button } from "@/components/ui/button"
import {
  SidebarContent,
  SidebarFooter as SbFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  SidebarSeparator,
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
  ArrowUpRight,
  Bot,
  Brain,
  ChevronDown,
  ChevronRight,
  Search,
  SlidersHorizontal,
  SquarePen,
  Trash2,
  X,
} from "lucide-react"
import {
  ArchivedSectionHeader,
  ConfirmDialog,
  ContextMenu,
  EmptyState,
  ErrorBanner,
  RepoSectionHeader,
  SessionListItem,
  SidebarActionRow,
  SidebarFooter,
  SidebarProjectFilter,
  SidebarResumeError,
  SidebarSkeleton,
  type ContextMenuItem,
} from "../molecules"
import { useQueryClient } from "@tanstack/react-query"
import { SESSIONS_KEY, useSessions } from "../../hooks/useSessions"
import { useWorkspaceStatuses } from "../../hooks/useWorkspaceStatuses"
import { useGitStatuses } from "../../hooks/useGitStatuses"
import { useIndexedRepos } from "../../hooks/useIndexedRepos"
import { useSessionSidebarGroups } from "../../hooks/useSessionSidebarGroups"
import {
  discardIsolatedSession,
  resumeSession,
  toInvokeError,
} from "../../lib/tauri"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import { isSessionNotFoundError } from "../../lib/sessions"
import { isDefaultSessionTitle, type SessionMeta } from "../../lib/types"
import { cn } from "../../lib/utils"
import { persistUiState, useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
} from "../../stores/layoutConstants"

const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/i.test(navigator.platform)

type SessionSidebarProps = {
  onOpenSearch: () => void
}

export const SessionSidebar = ({ onOpenSearch }: SessionSidebarProps) => {
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
  const narrow = viewport !== "wide"
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
  const [collapsedRepos, setCollapsedRepos] = useState<Record<string, boolean>>({})
  const [archivedCollapsed, setArchivedCollapsed] = useState(true)
  const [dragging, setDragging] = useState(false)
  const [repoMenu, setRepoMenu] = useState<{
    cwd: string
    position: { x: number; y: number }
  } | null>(null)
  const [deleteProject, setDeleteProject] = useState<{
    cwd: string
    label: string
    sessions: SessionMeta[]
  } | null>(null)
  const [deletingProject, setDeletingProject] = useState(false)
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
  const queryClient = useQueryClient()
  // Subagent child sessions render inside their parent's feed — the sidebar
  // lists only top-level agents.
  const sessions = useMemo(
    () => allSessions.filter((s) => !s.parent_id),
    [allSessions],
  )

  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId) ?? null,
    [sessions, activeSessionId],
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

  const { pinnedSessions, archivedSessions, repoGroups } = useSessionSidebarGroups(
    sessions,
    pinnedSessionIds,
    archivedSessionIds,
    {
      sort: sidebarProjectSort,
      visibility: sidebarProjectVisibility,
      activeSession,
    },
  )

  const handleCreate = useCallback(
    async (cwd?: string) => {
      // Collapse overlay immediately so the click feels responsive even when
      // create_session waits on isolation / engine work. Do not await —
      // mutation onSuccess selects the new session; awaiting here left the
      // sidebar button dead until create + post-select git/`gh` finished.
      if (narrow) setSidebarCollapsed(true)
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
      // Empty New Agent drafts should open as a single-tab composition
      // (Cursor empty agent) — prune sibling Changes/Files tabs + collapse split.
      const sessions =
        queryClient.getQueryData<SessionMeta[]>(SESSIONS_KEY) ?? []
      const meta = sessions.find((s) => s.id === id)
      const pristineDraft =
        !!meta &&
        isDefaultSessionTitle(meta.title) &&
        !meta.base_cwd &&
        !meta.workspace_id
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
      for (const s of group.sessions) ids.add(s.id)
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
    archivedCollapsed,
    archivedSessions,
  ])
  const sessionIdsForPoll = useMemo(
    () => [...statusPollIds],
    [statusPollIds],
  )
  const sessionCwdsForPoll = useMemo(
    () =>
      sessions
        .filter((s) => statusPollIds.has(s.id))
        .map((s) => ({ id: s.id, cwd: s.cwd })),
    [sessions, statusPollIds],
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
          icon: collapsedRepos[repoMenu.cwd] ? ChevronRight : ChevronDown,
          onSelect: () => toggleRepo(repoMenu.cwd),
        },
        { type: "separator" },
        {
          type: "item",
          label: "Delete project & chats…",
          icon: Trash2,
          danger: true,
          onSelect: () => {
            const group = repoGroups.find((g) => g.cwd === repoMenu.cwd)
            if (!group) return
            setDeleteProject({
              cwd: group.cwd,
              label: group.label,
              sessions: [...group.sessions],
            })
          },
        },
      ]
    : []

  const handleDeleteProject = useCallback(async () => {
    if (!deleteProject) return
    setDeletingProject(true)
    const ids = deleteProject.sessions.map((s) => s.id)
    const cwd = deleteProject.cwd
    try {
      for (const session of deleteProject.sessions) {
        const isolated =
          !!session.base_cwd &&
          session.base_cwd !== session.cwd &&
          !!session.workspace_id
        if (isolated) {
          try {
            await discardIsolatedSession(session.id)
          } catch {
            // Best-effort — delete still proceeds.
          }
        }
        await deleteSession(session.id)
      }
      const state = useAppStore.getState()
      const pinnedSessionIds = state.pinnedSessionIds.filter(
        (id) => !ids.includes(id),
      )
      const archivedSessionIds = state.archivedSessionIds.filter(
        (id) => !ids.includes(id),
      )
      const recentCwds = state.recentCwds.filter((p) => p !== cwd)
      useAppStore.setState({
        pinnedSessionIds,
        archivedSessionIds,
        recentCwds,
      })
      void persistUiState({ pinnedSessionIds, archivedSessionIds, recentCwds })
      setDeleteProject(null)
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    } finally {
      setDeletingProject(false)
    }
  }, [deleteProject, deleteSession, pushToast])

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
          "relative flex h-full shrink-0 flex-col overflow-hidden bg-sidebar",
          !dragging &&
            "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)] motion-reduce:transition-none",
          collapsed
            ? "w-0 border-r-0 opacity-0 pointer-events-none"
            : "border-r border-sidebar-border opacity-100",
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
            aria-valuemin={SIDEBAR_MIN_WIDTH}
            aria-valuemax={SIDEBAR_MAX_WIDTH}
            aria-valuenow={sidebarWidth}
            tabIndex={0}
            onPointerDown={handleSashDown}
            onDoubleClick={handleSashDoubleClick}
            onKeyDown={handleSashKeyDown}
            className={cn(
              "sash-line-transition absolute -right-[5px] inset-y-0 z-10 w-2.5 cursor-col-resize",
              "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
              // Quiet sash: white-alpha hover only — never accent (Feel: Quiet chrome).
              "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_12%,transparent)]",
              dragging && "after:bg-stroke-1",
            )}
          />
        ) : null}

        {/* ── SidebarHeader: narrow overlay close bar + action rows + repo label ── */}
        <SidebarHeader className="p-0 gap-0">
          {narrow && expanded ? (
            // Full-width overlay only — wide mode keeps the existing header
            // (backdrop click is enough at side-by-side width; discoverability
            // requires an explicit close control once the sidebar fills the
            // chat area).
            <div className="flex h-[var(--header-height)] shrink-0 items-center justify-between border-b border-sidebar-border px-4">
              <span className="text-sm text-ink-muted">Sessions</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="Close sidebar"
                title="Close sidebar"
                onClick={() => setSidebarCollapsed(true)}
                className={cn(
                  "text-ink-muted hover:bg-fill-4 hover:text-ink",
                  "opacity-50 hover:opacity-80",
                )}
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </div>
          ) : null}

          {/* Action rows: New Agent / Search / separator / nav links */}
          <div className="flex flex-col gap-0.5 pt-2 pb-2">
            <SidebarMenu className="gap-0.5">
              <SidebarMenuItem>
                <SidebarActionRow
                  icon={SquarePen}
                  label="New Agent"
                  kbd={isMac ? "⌘N" : "Ctrl+N"}
                  onClick={() => void handleCreate()}
                  disabled={isCreating}
                  loading={isCreating}
                />
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarActionRow
                  icon={Search}
                  label="Search"
                  kbd={isMac ? "⌘K" : "Ctrl+K"}
                  onClick={() => {
                    onOpenSearch()
                    if (narrow) setSidebarCollapsed(true)
                  }}
                />
              </SidebarMenuItem>
            </SidebarMenu>

            <SidebarSeparator className="mx-0 my-0" />

            <SidebarMenu className="gap-0.5">
              {AUTOMATIONS_UI_ENABLED ? (
                <SidebarMenuItem>
                  <SidebarActionRow
                    icon={Bot}
                    label="Automations"
                    trailingIcon={ArrowUpRight}
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
                  trailingIcon={ArrowUpRight}
                  onClick={() => {
                    setRoute("memory")
                    if (narrow) setSidebarCollapsed(true)
                  }}
                />
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarActionRow
                  icon={SlidersHorizontal}
                  label="Customize"
                  onClick={() => {
                    setRoute("customize")
                    if (narrow) setSidebarCollapsed(true)
                  }}
                />
              </SidebarMenuItem>
            </SidebarMenu>
          </div>

          {/* "Repositories" label with filter / search icons */}
          <div className="group/label flex items-center gap-1 px-2 pb-1">
            <SidebarGroupLabel className="h-6 flex-1 px-0 text-xs font-normal tracking-[var(--tracking-caption)] text-ink-muted">
              Repositories
            </SidebarGroupLabel>
            <SidebarProjectFilter
              sort={sidebarProjectSort}
              visibility={sidebarProjectVisibility}
              onSortChange={setSidebarProjectSort}
              onVisibilityChange={setSidebarProjectVisibility}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Search agents"
              title="Search agents"
              onClick={onOpenSearch}
              className={cn(
                "h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink",
                "transition-opacity duration-[var(--duration-fast)]",
                // Reveal on hover; when filter is non-default stay fully visible.
                sidebarProjectSort !== "recency" ||
                  sidebarProjectVisibility !== "all"
                  ? "opacity-100"
                  : "opacity-0 group-hover/label:opacity-100 group-focus-within/label:opacity-100 focus-visible:opacity-100",
              )}
            >
              <Search className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </div>
        </SidebarHeader>

        {error ? (
          <div className="px-2 pb-2">
            <ErrorBanner message={error} />
          </div>
        ) : null}

        {/* ── SidebarContent: scrollable session list ── */}
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
            <div className="flex flex-col gap-2">
              {pinnedSessions.length > 0 ? (
                <SidebarGroup className="p-0 gap-0">
                  <SidebarGroupLabel className="h-6 px-2 text-xs font-normal tracking-[var(--tracking-caption)] text-ink-muted">
                    Pinned
                  </SidebarGroupLabel>
                  <SidebarMenu className="gap-px">
                    {pinnedSessions.map((session) => (
                      <SidebarMenuItem key={session.id}>
                        <SessionListItem
                          session={session}
                          isActive={session.id === activeSessionId}
                          errorMessage={rowErrors[session.id]}
                          workspaceStatus={workspaceStatuses[session.id]}
                          gitStatus={gitStatuses[session.id]}
                          pinned
                          onSelect={handleSelectRow}
                          onRename={renameSession}
                          onDelete={deleteSession}
                          onNewAgentInRepo={handleNewAgentInRepo}
                          onTogglePin={toggleSessionPinned}
                          onSetArchived={handleSetArchived}
                        />
                      </SidebarMenuItem>
                    ))}
                  </SidebarMenu>
                </SidebarGroup>
              ) : null}

              {repoGroups.map((group) => (
                <SidebarGroup key={group.cwd} className="p-0 gap-0">
                  <div
                    onContextMenu={(e) => handleRepoContextMenu(e, group.cwd)}
                  >
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
                      {group.sessions.map((session) => (
                        <SidebarMenuItem key={session.id}>
                          <SessionListItem
                            session={session}
                            isActive={session.id === activeSessionId}
                            errorMessage={rowErrors[session.id]}
                            workspaceStatus={workspaceStatuses[session.id]}
                            gitStatus={gitStatuses[session.id]}
                            onSelect={handleSelectRow}
                            onRename={renameSession}
                            onDelete={deleteSession}
                            onNewAgentInRepo={handleNewAgentInRepo}
                            onTogglePin={toggleSessionPinned}
                            onSetArchived={handleSetArchived}
                          />
                        </SidebarMenuItem>
                      ))}
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
                      {archivedSessions.map((session) => (
                        <SidebarMenuItem key={session.id}>
                          <SessionListItem
                            session={session}
                            isActive={session.id === activeSessionId}
                            errorMessage={rowErrors[session.id]}
                            workspaceStatus={workspaceStatuses[session.id]}
                            gitStatus={gitStatuses[session.id]}
                            archived
                            onSelect={handleSelectRow}
                            onRename={renameSession}
                            onDelete={deleteSession}
                            onNewAgentInRepo={handleNewAgentInRepo}
                            onTogglePin={toggleSessionPinned}
                            onSetArchived={handleSetArchived}
                          />
                        </SidebarMenuItem>
                      ))}
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

        {/* ── SidebarFooter: theme toggle + settings ── */}
        <SbFooter className="p-0 gap-0">
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

        <ContextMenu
          position={repoMenu?.position ?? null}
          items={repoMenuItems}
          onClose={() => setRepoMenu(null)}
        />
        <ConfirmDialog
          open={!!deleteProject}
          title="Delete project & chats?"
          description={
            deleteProject
              ? `Delete "${deleteProject.label}" and its ${deleteProject.sessions.length} chat${
                  deleteProject.sessions.length === 1 ? "" : "s"
                }? This cannot be undone.`
              : undefined
          }
          confirmLabel="Delete"
          danger
          isLoading={deletingProject}
          onConfirm={() => void handleDeleteProject()}
          onCancel={() => {
            if (!deletingProject) setDeleteProject(null)
          }}
        />
      </aside>
    </>
  )
}
