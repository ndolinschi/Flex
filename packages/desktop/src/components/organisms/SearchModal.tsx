import { useEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { Search } from "lucide-react"
import { FuzzySessionRow } from "../molecules"
import { useSessions } from "../../hooks/useSessions"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import type { SessionMeta } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"

type SearchModalProps = {
  open: boolean
  onClose: () => void
}

type SearchGroup = {
  cwd: string
  label: string
  sessions: SessionMeta[]
  latestMs: number
}

const groupByRepo = (sessions: SessionMeta[]): SearchGroup[] => {
  const groups = new Map<string, SearchGroup>()
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

/**  "Search Agents" modal — same chrome as CommandPalette. */
export const SearchModal = ({ open, onClose }: SearchModalProps) => {
  const [query, setQuery] = useState("")
  const [activeIndex, setActiveIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const { sessions: allSessions } = useSessions()
  // Match the sidebar: subagent child sessions are not directly searchable.
  const sessions = useMemo(
    () => allSessions.filter((s) => !s.parent_id),
    [allSessions],
  )
  const setRoute = useAppStore((s) => s.setRoute)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)

  const handleSelectSession = async (id: string) => {
    // Mirrors SessionSidebar.handleSelect's happy path (resume then activate)
    // without duplicating its per-row error-banner state — a console.error
    // here is enough for a modal action the user can just retry.
    try {
      await resumeSession(id)
      setActiveSessionId(id)
      setRoute("chat")
    } catch (err) {
      console.error("resume_session failed", id, toInvokeError(err))
    }
  }

  const filteredSessions = useMemo(() => {
    const q = query.trim()
    if (!q) return sessions
    return sessions
      .map((s) => ({ s, score: fuzzyScore(q, sessionLabel(s)) }))
      .filter((r): r is { s: SessionMeta; score: number } => r.score !== null)
      .sort((a, b) => a.score - b.score)
      .map((r) => r.s)
  }, [sessions, query])

  const groups = useMemo(() => groupByRepo(filteredSessions), [filteredSessions])
  const flatSessions = useMemo(() => groups.flatMap((g) => g.sessions), [groups])

  useEffect(() => {
    setActiveIndex(0)
  }, [query, open])

  useEffect(() => {
    if (open) setQuery("")
  }, [open])

  useEffect(() => {
    if (!open) return
    const el = inputRef.current
    if (el) requestAnimationFrame(() => el.focus())
  }, [open])

  useEffect(() => {
    if (!open) return

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onClose()
        return
      }
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setActiveIndex((i) => Math.min(i + 1, flatSessions.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setActiveIndex((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Enter") {
        e.preventDefault()
        const session = flatSessions[activeIndex]
        if (session) {
          void handleSelectSession(session.id)
          onClose()
        }
      }
    }

    window.addEventListener("keydown", handleKey)
    return () => window.removeEventListener("keydown", handleKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, onClose, flatSessions, activeIndex])

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${activeIndex}"]`,
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [activeIndex])

  if (!open) return null

  let runningIndex = -1

  return createPortal(
    <div
      className="fixed inset-0 z-[300] flex justify-center bg-black/20 animate-backdrop-in"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className={cn(
          "mt-[10vh] flex h-fit w-[600px] max-w-[90vw] flex-col overflow-hidden",
          "rounded-lg bg-panel shadow-[var(--shadow-popover)] animate-tray-in",
        )}
        role="dialog"
        aria-label="Search agents"
      >
        <div className="flex items-center gap-1.5 border-b border-stroke-3 px-3 py-2.5">
          <Search className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search Agents…"
            aria-label="Search agents input"
            className="w-full bg-transparent text-[13px] text-ink outline-none placeholder:text-ink-faint"
          />
        </div>

        <div ref={listRef} className="max-h-[320px] overflow-y-auto py-1">
          {groups.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-ink-faint">
              No agents found
            </p>
          ) : (
            groups.map((group) => (
              <div key={group.cwd} className="py-1">
                <p className="px-3 py-1 text-[11px] font-medium text-ink-faint">
                  {group.label}
                </p>
                {group.sessions.map((session) => {
                  runningIndex += 1
                  const index = runningIndex
                  return (
                    <FuzzySessionRow
                      key={session.id}
                      index={index}
                      active={index === activeIndex}
                      label={sessionLabel(session)}
                      query={query}
                      updatedAtMs={session.updated_at_ms}
                      onHover={() => setActiveIndex(index)}
                      onActivate={() => {
                        void handleSelectSession(session.id)
                        onClose()
                      }}
                    />
                  )
                })}
              </div>
            ))
          )}
        </div>
      </div>
    </div>,
    document.body,
  )
}
