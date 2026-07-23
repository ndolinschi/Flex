import { useEffect, useMemo, useState } from "react"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { HighlightedLabel } from "../atoms"
import { useSessions } from "../../hooks/useSessions"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { groupByRepo } from "../../lib/sessionGrouping"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import { sessionLabel } from "../../lib/types"
import { formatRelativeTime } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"

type SearchModalProps = {
  open: boolean
  onClose: () => void
}

export const SearchModal = ({ open, onClose }: SearchModalProps) => {
  const [query, setQuery] = useState("")

  const { sessions: allSessions } = useSessions()
  const sessions = useMemo(
    () => allSessions.filter((s) => !s.parent_id),
    [allSessions],
  )
  const setRoute = useAppStore((s) => s.setRoute)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)

  const handleSelectSession = async (id: string) => {
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
          a.score! - b.score! || b.session.updated_at_ms - a.session.updated_at_ms,
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
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      <DialogContent
        showCloseButton={false}
        className="top-[10vh] max-w-[min(100%,560px)] translate-y-0 gap-0 overflow-hidden bg-panel p-0 shadow-popover sm:max-w-[560px]"
      >
        <DialogHeader className="sr-only">
          <DialogTitle>Search agents</DialogTitle>
          <DialogDescription>
            Search and switch between agent sessions.
          </DialogDescription>
        </DialogHeader>

        <Command
          shouldFilter={false}
          className="rounded-none bg-transparent p-0"
        >
          <div className="border-b border-stroke-3">
            <CommandInput
              value={query}
              onValueChange={setQuery}
              placeholder="Search Agents…"
              autoFocus
            />
          </div>
          <CommandList className="max-h-[min(320px,60vh)] py-1">
            <CommandEmpty className="py-6 text-center text-sm text-ink-faint">
              No agents found
            </CommandEmpty>
            {groups.map((group) => (
              <CommandGroup key={group.cwd} heading={group.label}>
                {group.sessions.map((session) => {
                  const label = sessionLabel(session)
                  return (
                    <CommandItem
                      key={session.id}
                      value={session.id}
                      onSelect={() => {
                        void handleSelectSession(session.id)
                        onClose()
                      }}
                    >
                      <span className="min-w-0 flex-1 truncate">
                        <HighlightedLabel label={label} query={query} />
                      </span>
                      <span className="shrink-0 text-xs text-ink-faint">
                        {formatRelativeTime(session.updated_at_ms)}
                      </span>
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  )
}
