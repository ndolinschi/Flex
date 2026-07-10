import { useEffect, useRef, useState } from "react"
import { List, Plus, Terminal as TerminalIcon, X } from "lucide-react"
import { Button, IconButton, ScrollArea } from "../atoms"
import { ConfirmDialog } from "../molecules"
import { useTerminal } from "../../hooks/useTerminal"
import { terminalCreate, terminalKill } from "../../lib/tauri"
import { dropTerminalBuffer, ensureTerminalBus } from "../../lib/terminalBus"
import { useSessions } from "../../hooks/useSessions"
import { useAppStore } from "../../stores/appStore"
import { basename, cn } from "../../lib/utils"

/* ── Elapsed time ticker ──────────────────────────────────────────────── */

const formatElapsed = (createdAtMs: number, nowMs: number): string => {
  const diff = nowMs - createdAtMs
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return "now"
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  return `${hours}h`
}

const useNowTicker = (intervalMs: number): number => {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs)
    return () => clearInterval(id)
  }, [intervalMs])
  return now
}

/* ── One xterm instance ───────────────────────────────────────────────── */

const TerminalInstance = ({ id, active }: { id: string; active: boolean }) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const { fit } = useTerminal(id, containerRef)

  useEffect(() => {
    if (active) fit()
  }, [active, fit])

  return (
    <div className={cn("glass-terminal-wrapper h-full w-full", !active && "hidden")}>
      <div ref={containerRef} className="h-full w-full" />
    </div>
  )
}

/* ── Terminal list row ────────────────────────────────────────────────── */

const TerminalRow = ({
  title,
  createdAtMs,
  selected,
  now,
  onSelect,
  onRequestClose,
}: {
  title: string
  createdAtMs: number
  selected: boolean
  now: number
  onSelect: () => void
  onRequestClose: () => void
}) => {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault()
          onSelect()
        }
      }}
      className={cn(
        "group mx-1 flex cursor-pointer items-center gap-1.5 rounded-sm px-2 py-1 text-sm",
        selected ? "bg-fill-5 text-ink" : "hover:bg-fill-4",
      )}
    >
      <TerminalIcon className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{title}</span>
      <span className="shrink-0 text-xs text-ink-muted group-hover:hidden">
        {formatElapsed(createdAtMs, now)}
      </span>
      <button
        type="button"
        aria-label="Close terminal"
        title="Close Terminal"
        onClick={(e) => {
          e.stopPropagation()
          onRequestClose()
        }}
        className={cn(
          "hidden h-4 w-4 shrink-0 items-center justify-center rounded-sm text-ink-muted",
          "hover:bg-fill-4 hover:text-ink group-hover:flex",
        )}
      >
        <X className="h-3 w-3" aria-hidden />
      </button>
    </div>
  )
}

/* ── Terminal tab ─────────────────────────────────────────────────────── */

/** Cursor-style Terminal right-panel tab: terminal list + xterm instances.
 * Stays mounted when inactive (parent hides via display:none). */
export const TerminalTab = ({ active }: { active: boolean }) => {
  const terminals = useAppStore((s) => s.terminals)
  const activeTerminalId = useAppStore((s) => s.activeTerminalId)
  const terminalListVisible = useAppStore((s) => s.terminalListVisible)
  const addTerminal = useAppStore((s) => s.addTerminal)
  const removeTerminal = useAppStore((s) => s.removeTerminal)
  const setActiveTerminalId = useAppStore((s) => s.setActiveTerminalId)
  const toggleTerminalListVisible = useAppStore((s) => s.toggleTerminalListVisible)

  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const { sessions } = useSessions()
  const activeSession = sessions.find((s) => s.id === activeSessionId)

  const [pendingClose, setPendingClose] = useState<string | null>(null)
  const [closing, setClosing] = useState(false)
  const now = useNowTicker(30_000)

  // Register the output listener before any terminal can be created, so the
  // shell's first output (the prompt) is buffered even if no xterm instance
  // has mounted yet.
  useEffect(() => {
    ensureTerminalBus()
  }, [])

  const activeTerminal = terminals.find((t) => t.id === activeTerminalId)

  const handleNewTerminal = async () => {
    const cwd = activeSession?.cwd
    const info = await terminalCreate(cwd)
    addTerminal({
      id: info.id,
      title: basename(info.cwd) || "Terminal",
      cwd: info.cwd,
      createdAtMs: info.createdAtMs,
    })
    setActiveTerminalId(info.id)
  }

  const handleConfirmClose = async () => {
    if (!pendingClose) return
    const id = pendingClose
    setClosing(true)
    try {
      await terminalKill(id)
      removeTerminal(id)
      dropTerminalBuffer(id)
      if (activeTerminalId === id) {
        const remaining = terminals.filter((t) => t.id !== id)
        setActiveTerminalId(remaining[0]?.id ?? null)
      }
    } finally {
      setClosing(false)
      setPendingClose(null)
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-3">
        <span className="min-w-0 flex-1 truncate text-base text-ink">
          {activeTerminal ? basename(activeTerminal.cwd) || "Terminal" : "Terminal"}
        </span>
        <IconButton
          label={terminalListVisible ? "Hide Terminal List" : "Show Terminal List"}
          onClick={toggleTerminalListVisible}
          className="h-6 w-6"
        >
          <List className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      {/* Body */}
      <div className="flex min-h-0 flex-1">
        {terminalListVisible && terminals.length > 0 ? (
          <div className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
            <div className="flex h-6 shrink-0 items-center justify-between px-2 text-xs text-ink-muted">
              <span>
                {terminals.length === 1
                  ? "1 Terminal"
                  : `${terminals.length} Terminals`}
              </span>
              <IconButton
                label="New Terminal"
                onClick={() => void handleNewTerminal()}
                className="h-5 w-5"
              >
                <Plus className="h-3 w-3" aria-hidden />
              </IconButton>
            </div>
            <p className="px-2 pt-1 text-xs text-ink-faint">Workspace</p>
            <ScrollArea className="min-h-0 flex-1 py-1">
              {terminals.map((t) => (
                <TerminalRow
                  key={t.id}
                  title={t.title}
                  createdAtMs={t.createdAtMs}
                  selected={t.id === activeTerminalId}
                  now={now}
                  onSelect={() => setActiveTerminalId(t.id)}
                  onRequestClose={() => setPendingClose(t.id)}
                />
              ))}
            </ScrollArea>
          </div>
        ) : null}

        <div className="relative min-h-0 flex-1">
          {terminals.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 h-full">
              <p className="text-base text-ink-secondary">No terminals</p>
              <p className="max-w-[280px] text-center text-sm text-ink-muted">
                Create a terminal to run commands in this workspace.
              </p>
              <Button
                variant="secondary"
                size="sm"
                className="mt-2"
                onClick={() => void handleNewTerminal()}
              >
                <Plus className="h-3 w-3" aria-hidden /> New Terminal
              </Button>
            </div>
          ) : (
            terminals.map((t) => (
              <TerminalInstance
                key={t.id}
                id={t.id}
                active={active && t.id === activeTerminalId}
              />
            ))
          )}
        </div>
      </div>

      <ConfirmDialog
        open={pendingClose !== null}
        title="Close Terminal?"
        description="This terminal has a running process. Closing it will terminate the process."
        confirmLabel="Close Terminal"
        danger
        isLoading={closing}
        onConfirm={() => void handleConfirmClose()}
        onCancel={() => setPendingClose(null)}
      />
    </div>
  )
}
