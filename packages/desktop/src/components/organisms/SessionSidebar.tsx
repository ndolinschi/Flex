import {
  useMemo,
  useRef,
  useEffect,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  Bot,
  Moon,
  Search,
  Settings,
  SlidersHorizontal,
  SquarePen,
  Sun,
} from "lucide-react"
import { IconButton, ScrollArea, Skeleton, Spinner, TextInput } from "../atoms"
import {
  EmptyState,
  ErrorBanner,
  RepoSectionHeader,
  SessionListItem,
  SidebarActionRow,
} from "../molecules"
import { useSessions } from "../../hooks/useSessions"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import type { SessionMeta } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"

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

export const SessionSidebar = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const searchOpen = useAppStore((s) => s.sidebarSearchOpen)
  const searchQuery = useAppStore((s) => s.sidebarSearchQuery)
  const setSidebarSearchQuery = useAppStore((s) => s.setSidebarSearchQuery)
  const toggleSidebarSearch = useAppStore((s) => s.toggleSidebarSearch)
  const setSidebarSearchOpen = useAppStore((s) => s.setSidebarSearchOpen)
  const theme = useAppStore((s) => s.theme)
  const toggleTheme = useAppStore((s) => s.toggleTheme)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const sidebarWidth = useAppStore((s) => s.sidebarWidth)
  const setSidebarWidth = useAppStore((s) => s.setSidebarWidth)
  const [selectError, setSelectError] = useState<string | null>(null)
  const [collapsedRepos, setCollapsedRepos] = useState<Record<string, boolean>>({})
  const [dragging, setDragging] = useState(false)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const {
    sessions,
    isLoading,
    error,
    newAgent,
    renameSession,
    deleteSession,
    isCreating,
  } = useSessions()

  useEffect(() => {
    if (searchOpen) searchInputRef.current?.focus()
  }, [searchOpen])

  const handleCreate = async (cwd?: string) => {
    await newAgent(cwd)
  }

  const handleSelect = async (id: string) => {
    setSelectError(null)
    try {
      await resumeSession(id)
      setActiveSessionId(id)
      setRoute("chat")
    } catch (err) {
      setSelectError(toInvokeError(err))
    }
  }

  const filteredSessions = useMemo(() => {
    const q = searchQuery.trim().toLowerCase()
    if (!q) return sessions
    return sessions.filter((s) =>
      sessionLabel(s).toLowerCase().includes(q),
    )
  }, [sessions, searchQuery])

  const repoGroups = useMemo(
    () => groupByRepo(filteredSessions),
    [filteredSessions],
  )

  const toggleRepo = (cwd: string) =>
    setCollapsedRepos((prev) => ({ ...prev, [cwd]: !prev[cwd] }))

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

  return (
    <aside
      style={collapsed ? undefined : { width: sidebarWidth }}
      className={cn(
        "relative flex h-full shrink-0 flex-col overflow-hidden bg-surface",
        !dragging &&
          "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
        collapsed
          ? "w-0 border-r-0 opacity-0 pointer-events-none"
          : "border-r border-stroke-3 opacity-100",
      )}
      aria-hidden={collapsed}
      aria-label="Sessions sidebar"
    >
      {collapsed ? null : (
        <div
          role="separator"
          aria-orientation="vertical"
          onPointerDown={handleSashDown}
          className={cn(
            "absolute inset-y-0 right-0 z-10 w-1 cursor-ew-resize",
            "transition-colors duration-[var(--duration-fast)] hover:bg-stroke-2",
            dragging && "bg-stroke-2",
          )}
        />
      )}

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
          onClick={() => toggleSidebarSearch()}
        />
        <SidebarActionRow
          icon={Bot}
          label="Automations"
          onClick={() => setRoute("automations")}
        />
        <SidebarActionRow
          icon={SlidersHorizontal}
          label="Customize"
          onClick={() => setRoute("customize")}
        />
      </div>

      {searchOpen ? (
        <div className="px-2 pb-2">
          <TextInput
            ref={searchInputRef}
            value={searchQuery}
            onChange={(e) => setSidebarSearchQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setSidebarSearchOpen(false)
            }}
            placeholder="Search agents"
            aria-label="Search agents"
            className="h-7 text-base"
          />
        </div>
      ) : null}

      <div className="group/label flex items-center gap-1 px-2 pb-1">
        <span className="min-w-0 flex-1 truncate px-2 text-sm text-ink-muted">
          Agents
        </span>
        <IconButton
          label="Search agents"
          className={cn(
            "h-6 w-6 opacity-0 transition-opacity duration-[var(--duration-fast)]",
            "group-hover/label:opacity-100",
          )}
          onClick={() => toggleSidebarSearch()}
        >
          <Search className="h-3 w-3" aria-hidden />
        </IconButton>
      </div>

      {error || selectError ? (
        <div className="px-2 pb-2">
          <ErrorBanner message={error ?? selectError ?? ""} />
        </div>
      ) : null}

      <ScrollArea className="flex-1 px-2 pb-2">
        {isLoading ? (
          <div className="flex flex-col gap-1 p-1">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-7 w-full rounded-sm" />
            ))}
          </div>
        ) : repoGroups.length === 0 ? (
          searchQuery ? (
            <p className="px-2 py-3 text-center text-sm text-ink-faint">
              No matching agents
            </p>
          ) : (
            <EmptyState
              title="No agents yet"
              description="Create an agent to start working on tasks."
              actionLabel="New Agent"
              onAction={() => void handleCreate()}
            />
          )
        ) : (
          <div className="flex flex-col gap-2">
            {repoGroups.map((group) => (
              <section key={group.cwd} className="flex flex-col gap-px">
                <RepoSectionHeader
                  label={group.label}
                  collapsed={!!collapsedRepos[group.cwd]}
                  onToggle={() => toggleRepo(group.cwd)}
                  onNewSession={() => void handleCreate(group.cwd)}
                />
                {!collapsedRepos[group.cwd]
                  ? group.sessions.map((session) => (
                      <SessionListItem
                        key={session.id}
                        session={session}
                        isActive={session.id === activeSessionId}
                        isRunning={!!streamingSessions[session.id]}
                        onSelect={(id) => void handleSelect(id)}
                        onRename={renameSession}
                        onDelete={deleteSession}
                      />
                    ))
                  : null}
              </section>
            ))}
          </div>
        )}
      </ScrollArea>

      <div className="flex items-center justify-end gap-0.5 border-t border-stroke-3 px-2 py-1.5">
        <IconButton
          label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          onClick={toggleTheme}
        >
          {theme === "dark" ? (
            <Sun className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <Moon className="h-3.5 w-3.5" aria-hidden />
          )}
        </IconButton>
        <IconButton label="Settings" onClick={() => setRoute("settings")}>
          <Settings className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      {isCreating ? (
        <div className="flex items-center gap-2 border-t border-stroke-3 px-4 py-2 text-xs text-ink-muted">
          <Spinner size="sm" />
          Creating…
        </div>
      ) : null}
    </aside>
  )
}
