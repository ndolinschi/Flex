import { useEffect, useMemo, useState } from "react"
import { HighlightedLabel } from "../atoms"
import { useSessions } from "../../hooks/useSessions"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { groupByRepo } from "../../lib/sessionGrouping"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import { sessionLabel } from "../../lib/types"
import { cn, formatRelativeTime } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command"

type SearchModalProps = {
  open: boolean
  onClose: () => void
}

/** "Search Agents" modal — same chrome as CommandPalette. */
export const SearchModal = ({ open, onClose }: SearchModalProps) => {
  const [query, setQuery] = useState("")

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
    // without duplicating its per-row error-banner state — log.error is enough
    // for a modal action the user can just retry.
    try {
      await resumeSession(id)
      setActiveSessionId(id)
      setRoute("chat")
    } catch (err) {
      log.error("session", "resume_session failed", {
        sessionId: id,
        error: toInvokeError(err),
      })
    }
  }

  const filteredSessions = useMemo(() => {
    const q = query.trim()
    if (!q) return sessions
    return sessions
      .map((s) => ({ session: s, score: fuzzyScore(q, sessionLabel(s)) }))
      .filter((x) => x.score !== null)
      .sort(
        (a, b) =>
          a.score! - b.score! ||
          b.session.updated_at_ms - a.session.updated_at_ms,
      )
      .map((x) => x.session)
  }, [sessions, query])

  const groups = useMemo(
    () => groupByRepo(filteredSessions),
    [filteredSessions],
  )

  useEffect(() => {
    if (open) setQuery("")
  }, [open])

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
      title="Search agents"
      description="Search Agents"
      className={cn("shadow-[var(--shadow-popover)]")}
    >
      <Command shouldFilter={false} className="rounded-lg bg-panel">
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder="Search Agents…"
          aria-label="Search agents input"
        />
        <CommandList className="py-1">
          <CommandEmpty>No agents found</CommandEmpty>
          {groups.map((group) => (
            <CommandGroup key={group.cwd} heading={group.label}>
              {group.sessions.map((session) => (
                <CommandItem
                  key={session.id}
                  value={session.id}
                  onSelect={() => {
                    void handleSelectSession(session.id)
                    onClose()
                  }}
                >
                  <span className="min-w-0 flex-1 truncate">
                    <HighlightedLabel
                      label={sessionLabel(session)}
                      query={query}
                    />
                  </span>
                  <CommandShortcut className="tracking-normal">
                    {formatRelativeTime(session.updated_at_ms)}
                  </CommandShortcut>
                </CommandItem>
              ))}
            </CommandGroup>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
