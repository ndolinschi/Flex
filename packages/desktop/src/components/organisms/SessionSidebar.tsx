import {
  useCallback,
  useMemo,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  ArrowUpRight,
  Bot,
  Brain,
  Search,
  SlidersHorizontal,
  SquarePen,
  Trash2,
  X,
} from "lucide-react"
import { IconButton, ScrollArea } from "../atoms"
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
import type { SessionMeta } from "../../lib/types"
import { cn } from "../../lib/utils"
import { persistUiState, useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"
import { SIDEBAR_DEFAULT_WIDTH } from "../../stores/layoutConstants"

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

  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId) ?? null,
    [sessions, activeSessionId],
  )

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
      await newAgent(cwd)
      if (narrow) setSidebarCollapsed(true)
    },
    [newAgent, narrow, setSidebarCollapsed],
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
      // Clear only this row's stale error banner/state — leave other rows alone.
      setSelectErrorId((prevId) => {
        if (prevId === id) {
          setSelectError(null)
          return null
        }
        return prevId
      })
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
      healNotFoundSession,
      narrow,
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

  const sessionIds = useMemo(() => sessions.map((s) => s.id), [sessions])
  const sessionCwds = useMemo(
    () => sessions.map((s) => ({ id: s.id, cwd: s.cwd })),
    [sessions],
  )
  // Poll only sessions the user can see (plus active + pinned), and pause
  // intervals entirely while the sidebar is collapsed — Changes tab keeps its
  // own observer on the shared git-status query key for the active session.
  const statusPollIds = useMemo(() => {
    const ids = new Set<string>()
    if (activeSessionId) ids.add(activeSessionId)
    for (const id of pinnedSessionIds) ids.add(id)
    for (const group of repoGroups) {
      if (collapsedRepos[group.cwd]) continue
      for (const s of group.sessions) ids.add(s.id)
    }
    if (!archivedCollapsed) {
      for (const s of archivedSessions) ids.add(s.id)
    }
    return ids
  }, [
    activeSessionId,
    pinnedSessionIds,
    repoGroups,
    collapsedRepos,
    archivedCollapsed,
    archivedSessions,
  ])
  const statusPollOptions = useMemo(
    () => ({
      pollingEnabled: !collapsed,
      pollIds: statusPollIds,
    }),
    [collapsed, statusPollIds],
  )
  const workspaceStatuses = useWorkspaceStatuses(sessionIds, statusPollOptions)
  const gitStatuses = useGitStatuses(sessionCwds, statusPollOptions)
  const repoCwds = useMemo(() => repoGroups.map((g) => g.cwd), [repoGroups])
  const indexedRepos = useIndexedRepos(repoCwds)

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
              "sash-line-transition absolute -right-[5px] inset-y-0 z-10 w-2.5 cursor-col-resize",
              "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
              // Sash hover = white-alpha focusBorder, never accent (Feel: Quiet chrome).
              "hover:after:bg-[color-mix(in_srgb,var(--color-text-1)_15%,transparent)]",
              dragging && "after:bg-stroke-1",
            )}
          />
        ) : null}

      {narrow && expanded ? (
        // Full-width overlay only — wide mode keeps the existing header
        // (backdrop click is enough at side-by-side width; discoverability
        // requires an explicit close control once the sidebar fills the
        // chat area).
        <div className="flex h-[var(--header-height)] shrink-0 items-center justify-between border-b border-stroke-3 px-4">
          <span className="text-sm text-ink-secondary">Sessions</span>
          <IconButton
            label="Close sidebar"
            onClick={() => setSidebarCollapsed(true)}
            className="h-6 w-6"
          >
            <X className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </div>
      ) : null}

      <div className="flex flex-col gap-0.5 px-2 pt-2 pb-2">
        <SidebarActionRow
          icon={SquarePen}
          label="New Agent"
          kbd={isMac ? "⌘N" : "Ctrl+N"}
          onClick={() => void handleCreate()}
          disabled={isCreating}
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
        {AUTOMATIONS_UI_ENABLED ? (
          <SidebarActionRow
            icon={Bot}
            label="Automations"
            trailingIcon={ArrowUpRight}
            onClick={() => {
              setRoute("automations")
              if (narrow) setSidebarCollapsed(true)
            }}
          />
        ) : null}
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
        <span className="min-w-0 flex-1 truncate text-xs tracking-[var(--tracking-caption)] text-ink-muted">
          Repositories
        </span>
        <SidebarProjectFilter
          sort={sidebarProjectSort}
          visibility={sidebarProjectVisibility}
          onSortChange={setSidebarProjectSort}
          onVisibilityChange={setSidebarProjectVisibility}
        />
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
          <SidebarSkeleton />
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
                <div className="flex h-6 w-full items-center gap-1.5 px-2 text-xs text-ink-secondary">
                  <span className="min-w-0 flex-1 truncate">Pinned</span>
                </div>
                {pinnedSessions.map((session) => (
                  <SessionListItem
                    key={session.id}
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
                    indexed={!!indexedRepos[group.cwd]}
                  />
                </div>
                {!collapsedRepos[group.cwd]
                  ? group.sessions.map((session) => (
                      <SessionListItem
                        key={session.id}
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
                        onSetArchived={setSessionArchived}
                      />
                    ))
                  : null}
              </section>
            ))}

            {archivedSessions.length > 0 ? (
              <section className="flex flex-col gap-px">
                <ArchivedSectionHeader
                  count={archivedSessions.length}
                  collapsed={archivedCollapsed}
                  onToggle={() => setArchivedCollapsed((prev) => !prev)}
                />
                {!archivedCollapsed
                  ? archivedSessions.map((session) => (
                      <SessionListItem
                        key={session.id}
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
        <SidebarResumeError
          message={selectError}
          onRetry={handleRetrySelect}
          onDismiss={handleDismissSelectError}
        />
      ) : null}

      <SidebarFooter
        theme={theme}
        onToggleTheme={toggleTheme}
        onOpenSettings={() => {
          setRoute("settings")
          if (narrow) setSidebarCollapsed(true)
        }}
        isCreating={isCreating}
      />

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
            ? `Delete “${deleteProject.label}” and its ${deleteProject.sessions.length} chat${
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
