import { useEffect, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { List, Plus, Terminal as TerminalIcon } from "lucide-react"

import {
  ConfirmDialog,
  EmptyState,
  PanelSideRail,
  PanelToolbar,
  panelChromeIconActiveClass,
  panelChromeIconClass,
} from "../../molecules"
import { agentTerminalId } from "../../../hooks/useGlobalSessionEvents"
import { terminalCreate, terminalKill, toInvokeError } from "../../../lib/tauri"
import { dropTerminalBuffer, ensureTerminalBus } from "../../../lib/terminalBus"
import { useSessions } from "../../../hooks/useSessions"
import { useAppStore, sessionScopeKey, type TerminalMeta } from "../../../stores/appStore"
import { basename, cn } from "../../../lib/utils"
import { AgentTerminalRow } from "./AgentTerminalRow"
import { TerminalInstance } from "./TerminalInstance"
import { TerminalRow } from "./TerminalRow"
import { useNowTicker } from "./time"
import { ScrollArea } from "@/components/ui/scroll-area"

const EMPTY_TERMINALS: TerminalMeta[] = []

export const TerminalTab = ({
  active,
  sessionId,
}: {
  active: boolean
  sessionId: string | null
}) => {
  const sessionKey = sessionScopeKey(sessionId)

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

  const agentId = sessionId ? agentTerminalId(sessionId) : null
  const hasAgentStream = useAppStore((s) =>
    agentId ? !!s.agentStreamSessions[agentId] : false,
  )

  const { sessions } = useSessions()
  const activeSession = sessions.find((s) => s.id === sessionId)

  const [pendingClose, setPendingClose] = useState<string | null>(null)
  const [closing, setClosing] = useState(false)
  const now = useNowTicker(30_000, active)
  const autoSpawnAttemptedRef = useRef(new Set<string>())

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
      if (info.cwdFallbackFrom) {
        useAppStore
          .getState()
          .pushToast(
            `Workspace folder missing — terminal opened in home instead of ${basename(info.cwdFallbackFrom) || info.cwdFallbackFrom}`,
            "error",
          )
      }
    } catch (err) {
      useAppStore
        .getState()
        .pushToast(toInvokeError(err) || "Could not open terminal", "error")
    }
  }

  useEffect(() => {
    if (!active) {
      autoSpawnAttemptedRef.current.delete(sessionKey)
      return
    }
    if (terminals.length > 0) return
    if (autoSpawnAttemptedRef.current.has(sessionKey)) return
    autoSpawnAttemptedRef.current.add(sessionKey)
    void handleNewTerminal()
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
    } catch (err) {
      useAppStore
        .getState()
        .pushToast(toInvokeError(err) || "Could not close terminal", "error")
    } finally {
      setClosing(false)
      setPendingClose(null)
    }
  }

  const titleLabel = isAgentSelected
    ? "Agent terminal"
    : activeTerminal
      ? activeTerminal.title || basename(activeTerminal.cwd) || "Terminal"
      : "Terminal"

  const listCount = terminals.length + (hasAgentStream ? 1 : 0)
  const listHeader =
    listCount === 0
      ? "Terminals"
      : `${listCount} Terminal${listCount === 1 ? "" : "s"}`

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PanelToolbar
        variant="elevated"
        aria-label="Terminal sessions"
        actions={
          <>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="New Terminal"
              title="New Terminal"
              onClick={() => void handleNewTerminal()}
              className={cn("h-6 w-6", panelChromeIconClass)}
            >
              <Plus className="size-3.5" strokeWidth={1.5} aria-hidden />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={
                terminalListVisible ? "Hide Terminal List" : "Show Terminal List"
              }
              title={
                terminalListVisible ? "Hide Terminal List" : "Show Terminal List"
              }
              onClick={toggleTerminalListVisible}
              className={cn(
                "h-6 w-6",
                panelChromeIconClass,
                terminalListVisible && panelChromeIconActiveClass,
              )}
            >
              <List className="size-3.5" strokeWidth={1.5} aria-hidden />
            </Button>
          </>
        }
      >
        <button
          type="button"
          className={cn(
            "flex h-6 min-w-0 max-w-full items-center gap-1.5 rounded-md px-2 text-base font-medium text-ink",
            "hover:bg-fill-4",
          )}
          title={titleLabel}
        >
          <span className="min-w-0 truncate">{titleLabel}</span>
        </button>
      </PanelToolbar>
      {isAgentSelected ? (
        <p className="shrink-0 truncate border-b border-stroke-3 px-2.5 py-1.5 text-sm text-ink-muted">
          Agent is using this terminal. It&apos;s read-only.
        </p>
      ) : null}

      <div className="flex min-h-0 flex-1">
        {terminalListVisible && (terminals.length > 0 || hasAgentStream) ? (
          <PanelSideRail
            width={160}
            header={<span className="tabular-nums">{listHeader}</span>}
          >
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              {hasAgentStream && agentId ? (
                <>
                  <p className="px-2.5 pb-1.5 text-xs text-ink-faint">Agent</p>
                  <AgentTerminalRow
                    selected={activeTerminalId === agentId}
                    onSelect={() => setActiveTerminalId(sessionKey, agentId)}
                  />
                </>
              ) : null}
              {terminals.length > 0 ? (
                <>
                  <p
                    className={cn(
                      "px-2.5 pb-1.5 text-xs text-ink-faint",
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
          </PanelSideRail>
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
          {activeTerminal ? (
            <TerminalInstance
              key={activeTerminal.id}
              id={activeTerminal.id}
              active={active}
            />
          ) : null}
          {isAgentSelected && agentId ? (
            <TerminalInstance
              key={agentId}
              id={agentId}
              active={active}
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
