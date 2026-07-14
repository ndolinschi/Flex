import { useEffect, useRef, useState } from "react"
import { List, Plus, Terminal as TerminalIcon } from "lucide-react"
import { IconButton, ScrollArea } from "../../atoms"
import { ConfirmDialog, EmptyState } from "../../molecules"
import { agentTerminalId } from "../../../hooks/useGlobalSessionEvents"
import { terminalCreate, terminalKill } from "../../../lib/tauri"
import { dropTerminalBuffer, ensureTerminalBus } from "../../../lib/terminalBus"
import { useSessions } from "../../../hooks/useSessions"
import { useAppStore, sessionScopeKey, type TerminalMeta } from "../../../stores/appStore"
import { basename, cn } from "../../../lib/utils"
import { AgentTerminalRow } from "./AgentTerminalRow"
import { TerminalInstance } from "./TerminalInstance"
import { TerminalRow } from "./TerminalRow"
import { useNowTicker } from "./time"

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_TERMINALS: TerminalMeta[] = []

/** Terminal right-panel tab: terminal list + xterm instances.
 * Scoped to the active session — switching sessions shows that session's own
 * terminal list. Only the active session's xterm instances stay mounted
 * (Wave 4); the terminal bus still buffers PTY output for other sessions so
 * remounting on switch restores scrollback without keeping N×xterm alive.
 * Stays mounted when inactive (parent hides via display:none).
 *
 * Opening the tab with zero workspace terminals auto-creates one PTY so the
 * user lands in a shell instead of an empty state. */
export const TerminalTab = ({ active }: { active: boolean }) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)

  const terminals = useAppStore(
    (s) => s.terminalsBySession[sessionKey] ?? EMPTY_TERMINALS,
  )
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
  const hasAgentStream = useAppStore((s) =>
    agentId ? !!s.agentStreamSessions[agentId] : false,
  )

  const { sessions } = useSessions()
  const activeSession = sessions.find((s) => s.id === activeSessionId)

  const [pendingClose, setPendingClose] = useState<string | null>(null)
  const [closing, setClosing] = useState(false)
  const now = useNowTicker(30_000)
  /** Sessions for which we've already tried default spawn this tab visit —
   * prevents StrictMode double-create and recreate-after-close while still
   * on the Terminal tab. Cleared when the tab becomes inactive. */
  const autoSpawnAttemptedRef = useRef(new Set<string>())

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
      const info = await terminalCreate(cwd)
      addTerminal(sessionKey, {
        id: info.id,
        title: basename(info.cwd) || "Terminal",
        cwd: info.cwd,
        createdAtMs: info.createdAtMs,
      })
      setActiveTerminalId(sessionKey, info.id)
    } catch {
      // Leave EmptyState / Plus button available for a manual retry.
    }
  }

  // Default: opening Terminal with no workspace PTYs creates one shell.
  useEffect(() => {
    if (!active) {
      autoSpawnAttemptedRef.current.delete(sessionKey)
      return
    }
    if (terminals.length > 0) return
    if (autoSpawnAttemptedRef.current.has(sessionKey)) return
    autoSpawnAttemptedRef.current.add(sessionKey)
    void handleNewTerminal()
    // eslint-disable-next-line react-hooks/exhaustive-deps -- spawn once per empty tab open
  }, [active, sessionKey, terminals.length, activeSession?.cwd])

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

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header — fixed chrome height so tab switches don't jump; agent
          subtitle lives on a separate non-chrome row below. */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 px-2">
        <span className="min-w-0 flex-1 truncate text-sm text-ink">
          {isAgentSelected
            ? "Agent terminal"
            : activeTerminal
              ? basename(activeTerminal.cwd) || "Terminal"
              : "Terminal"}
        </span>
        <div className="flex shrink-0 items-center gap-0.5">
          <IconButton
            label="New Terminal"
            onClick={() => void handleNewTerminal()}
            className="h-6 w-6"
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
          <IconButton
            label={terminalListVisible ? "Hide Terminal List" : "Show Terminal List"}
            onClick={toggleTerminalListVisible}
            className="h-6 w-6"
          >
            <List className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </div>
      </div>
      {isAgentSelected ? (
        <p className="shrink-0 truncate border-b border-stroke-3 px-2 py-1 text-sm text-ink-muted">
          Agent is using this terminal. It&apos;s read-only.
        </p>
      ) : null}

      {/* Body */}
      <div className="flex min-h-0 flex-1">
        {terminalListVisible && (terminals.length > 0 || hasAgentStream) ? (
          <div className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
            <div className="flex h-6 shrink-0 items-center px-2 text-xs text-ink-muted">
              <span>
                {terminals.length === 0
                  ? "Terminals"
                  : terminals.length === 1
                    ? "1 Terminal"
                    : `${terminals.length} Terminals`}
              </span>
            </div>
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              {hasAgentStream && agentId ? (
                <>
                  <p className="px-2 pb-1.5 text-xs text-ink-faint">Agent</p>
                  <AgentTerminalRow
                    selected={activeTerminalId === agentId}
                    onSelect={() => setActiveTerminalId(sessionKey, agentId)}
                  />
                </>
              ) : null}
              {/* Only label the "Workspace" section when there's something to
                  put under it — an empty heading with no rows is the exact
                  scaffolding noise this panel should avoid. */}
              {terminals.length > 0 ? (
                <>
                  <p
                    className={cn(
                      "px-2 pb-1.5 text-xs text-ink-faint",
                      hasAgentStream && agentId ? "pt-1.5" : undefined,
                    )}
                  >
                    Workspace
                  </p>
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
                </>
              ) : null}
            </ScrollArea>
          </div>
        ) : null}

        <div className="relative min-h-0 flex-1">
          {terminals.length === 0 && !hasAgentStream ? (
            <EmptyState
              className="h-full"
              icon={<TerminalIcon className="h-6 w-6" aria-hidden />}
              title="No terminal yet"
              actionLabel="New terminal"
              onAction={() => void handleNewTerminal()}
            />
          ) : null}
          {terminals.map((t) => (
            <TerminalInstance
              key={t.id}
              id={t.id}
              active={active && t.id === activeTerminalId}
            />
          ))}
          {hasAgentStream && agentId ? (
            <TerminalInstance
              key={agentId}
              id={agentId}
              active={active && agentId === activeTerminalId}
              readOnly
            />
          ) : null}
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
