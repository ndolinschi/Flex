import { useEffect, useMemo, useRef, useState } from "react"
import { Infinity as InfinityIcon, List, Plus, Terminal as TerminalIcon, X } from "lucide-react"
import { Button, IconButton, ScrollArea } from "../atoms"
import { ConfirmDialog } from "../molecules"
import { useTerminal } from "../../hooks/useTerminal"
import { agentTerminalId } from "../../hooks/useGlobalSessionEvents"
import { terminalCreate, terminalKill } from "../../lib/tauri"
import { dropTerminalBuffer, ensureTerminalBus } from "../../lib/terminalBus"
import { useSessions } from "../../hooks/useSessions"
import { useAppStore, sessionScopeKey, type TerminalMeta } from "../../stores/appStore"
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

const TerminalInstance = ({
  id,
  active,
  readOnly,
}: {
  id: string
  active: boolean
  readOnly?: boolean
}) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const { fit } = useTerminal(id, containerRef, active, { readOnly })

  useEffect(() => {
    if (active) fit()
  }, [active, fit])

  return (
    <div
      className={cn("glass-terminal-wrapper h-full w-full", !active && "hidden")}
      onMouseDown={() => {
        // Ensure keystrokes reach xterm after clicking the panel.
        const el = containerRef.current?.querySelector("textarea")
        el?.focus()
      }}
    >
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

/* ── Agent terminal list row ──────────────────────────────────────────── */

const AgentTerminalRow = ({
  selected,
  onSelect,
}: {
  selected: boolean
  onSelect: () => void
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
      <InfinityIcon className="h-3.5 w-3.5 shrink-0 text-yellow" aria-hidden />
      <span className="min-w-0 flex-1 truncate">Agent terminal</span>
    </div>
  )
}

/* ── Terminal tab ─────────────────────────────────────────────────────── */

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_TERMINALS: TerminalMeta[] = []

/** Terminal right-panel tab: terminal list + xterm instances.
 * Scoped to the active session — switching sessions shows that session's own
 * terminal list. All sessions' `TerminalInstance`s stay mounted (PTYs + xterm
 * scrollback survive session switches); only the active session's active
 * terminal is visible. Stays mounted when inactive (parent hides via
 * display:none). */
export const TerminalTab = ({ active }: { active: boolean }) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)

  const terminalsBySession = useAppStore((s) => s.terminalsBySession)
  const terminals = terminalsBySession[sessionKey] ?? EMPTY_TERMINALS
  const activeTerminalId = useAppStore(
    (s) => s.activeTerminalIdBySession[sessionKey] ?? null,
  )
  const terminalListVisible = useAppStore((s) => s.terminalListVisible)
  const addTerminal = useAppStore((s) => s.addTerminal)
  const removeTerminal = useAppStore((s) => s.removeTerminal)
  const setActiveTerminalId = useAppStore((s) => s.setActiveTerminalId)
  const toggleTerminalListVisible = useAppStore((s) => s.toggleTerminalListVisible)

  // Agent terminal (read-only, mirrors `exec_chunk` session-events). Lives in
  // the same `activeTerminalIdBySession` map as workspace terminals since its
  // id is just a string (`agent:${sessionId}`) — no PTY/terminalCreate behind it.
  const agentId = activeSessionId ? agentTerminalId(activeSessionId) : null
  const agentStreamSessions = useAppStore((s) => s.agentStreamSessions)
  const hasAgentStream = agentId ? !!agentStreamSessions[agentId] : false

  const { sessions } = useSessions()
  const activeSession = sessions.find((s) => s.id === activeSessionId)

  const [pendingClose, setPendingClose] = useState<string | null>(null)
  const [closing, setClosing] = useState(false)
  const now = useNowTicker(30_000)

  // Register the output listener before any terminal can be created, so the
  // shell's first output (the prompt) is buffered even if no xterm instance
  // has mounted yet.
  useEffect(() => {
    void ensureTerminalBus()
  }, [])

  const isAgentSelected = hasAgentStream && agentId !== null && activeTerminalId === agentId
  const activeTerminal = terminals.find((t) => t.id === activeTerminalId)

  const handleNewTerminal = async () => {
    try {
      await ensureTerminalBus()
      const cwd = activeSession?.cwd
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H2",
          location: "TerminalTab.tsx:handleNewTerminal",
          message: "creating terminal",
          data: { cwd: cwd ?? null, tabActive: active },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
      const info = await terminalCreate(cwd)
      addTerminal(sessionKey, {
        id: info.id,
        title: basename(info.cwd) || "Terminal",
        cwd: info.cwd,
        createdAtMs: info.createdAtMs,
      })
      setActiveTerminalId(sessionKey, info.id)
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H2",
          location: "TerminalTab.tsx:handleNewTerminal",
          message: "terminal created ok",
          data: { id: info.id, cwd: info.cwd },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
    } catch (err) {
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H2",
          location: "TerminalTab.tsx:handleNewTerminal",
          message: "terminal create failed",
          data: { error: String(err) },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
    }
  }

  const handleConfirmClose = async () => {
    if (!pendingClose) return
    const id = pendingClose
    setClosing(true)
    try {
      await terminalKill(id)
      removeTerminal(sessionKey, id)
      dropTerminalBuffer(id)
      if (activeTerminalId === id) {
        const remaining = terminals.filter((t) => t.id !== id)
        setActiveTerminalId(sessionKey, remaining[0]?.id ?? null)
      }
    } finally {
      setClosing(false)
      setPendingClose(null)
    }
  }

  // Union of every session's terminals — instances stay mounted across
  // session switches so PTYs and xterm scrollback survive (like the reference's
  // tmux-style persistence). Only the current session's active terminal
  // is visible; the rest render `hidden`.
  const allTerminals = useMemo(
    () => Object.values(terminalsBySession).flat(),
    [terminalsBySession],
  )

  // Union of every session's agent terminals — stays mounted across session
  // switches for the same reason as `allTerminals`, but keyed by the synthetic
  // `agent:${sessionId}` id (no PTY / terminalCreate behind it).
  const allAgentSessionKeys = useMemo(
    () => Object.keys(agentStreamSessions).filter((key) => agentStreamSessions[key]),
    [agentStreamSessions],
  )

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header */}
      <div className="flex min-h-[var(--header-height)] shrink-0 flex-col justify-center gap-0.5 border-b border-stroke-3 px-3 py-1.5">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-base text-ink">
            {isAgentSelected
              ? "Agent terminal"
              : activeTerminal
                ? basename(activeTerminal.cwd) || "Terminal"
                : "Terminal"}
          </span>
          <IconButton
            label={terminalListVisible ? "Hide Terminal List" : "Show Terminal List"}
            onClick={toggleTerminalListVisible}
            className="h-6 w-6"
          >
            <List className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </div>
        {isAgentSelected ? (
          <p className="truncate text-sm text-ink-muted">
            Agent is using this terminal. It&apos;s read-only.
          </p>
        ) : null}
      </div>

      {/* Body */}
      <div className="flex min-h-0 flex-1">
        {terminalListVisible && (terminals.length > 0 || hasAgentStream) ? (
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
            <ScrollArea className="min-h-0 flex-1 py-1">
              {hasAgentStream && agentId ? (
                <>
                  <p className="px-2 pt-1 text-xs text-ink-faint">Agent</p>
                  <AgentTerminalRow
                    selected={activeTerminalId === agentId}
                    onSelect={() => setActiveTerminalId(sessionKey, agentId)}
                  />
                </>
              ) : null}
              <p className="px-2 pt-1 text-xs text-ink-faint">Workspace</p>
              {terminals.map((t) => (
                <TerminalRow
                  key={t.id}
                  title={t.title}
                  createdAtMs={t.createdAtMs}
                  selected={t.id === activeTerminalId}
                  now={now}
                  onSelect={() => setActiveTerminalId(sessionKey, t.id)}
                  onRequestClose={() => setPendingClose(t.id)}
                />
              ))}
            </ScrollArea>
          </div>
        ) : null}

        <div className="relative min-h-0 flex-1">
          {terminals.length === 0 && !hasAgentStream ? (
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
          ) : null}
          {allTerminals.map((t) => (
            <TerminalInstance
              key={t.id}
              id={t.id}
              active={active && t.id === activeTerminalId}
            />
          ))}
          {allAgentSessionKeys.map((key) => (
            <TerminalInstance
              key={key}
              id={key}
              active={active && key === activeTerminalId}
              readOnly
            />
          ))}
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
