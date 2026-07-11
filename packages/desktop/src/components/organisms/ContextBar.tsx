import { useEffect, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge, XCircle } from "lucide-react"
import {
  gitCommit,
  gitIsRepo,
  gitPush,
  gitStatusSinceBaseline,
  isIsolated,
  toInvokeError,
  workspaceStatus,
} from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { useWorkspaceActions } from "../../hooks/useWorkspaceActions"
import { useModels } from "../../hooks/useModels"
import { cn, formatCost, formatTokens } from "../../lib/utils"
import { BranchPicker } from "../molecules/BranchPicker"
import { PopoverTray } from "../molecules/PopoverTray"
import { ProjectPicker } from "../molecules/ProjectPicker"
import { Button, TextInput } from "../atoms"

/** Fallback context budget used for the usage ring when the selected
 * model's own context window isn't known (the reference design shows a similar %). */
const CONTEXT_BUDGET_TOKENS = 200_000

const ContextRing = ({ fraction }: { fraction: number }) => {
  const radius = 5
  const circumference = 2 * Math.PI * radius
  const clamped = Math.min(1, Math.max(0, fraction))

  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
      <g transform="rotate(-90 7 7)">
        <circle
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeOpacity="0.28"
          strokeWidth="2"
        />
        <circle
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeDasharray={`${clamped * circumference} ${circumference}`}
        />
      </g>
    </svg>
  )
}

const UsageDetailRow = ({ label, value }: { label: string; value: string }) => (
  <div className="flex items-center justify-between gap-6">
    <span className="text-ink-muted">{label}</span>
    <span className="text-ink-secondary [font-variant-numeric:tabular-nums]">
      {value}
    </span>
  </div>
)

/** Context ring + % with a hover popover breaking down the last turn's usage. */
const UsageRing = ({ sessionId }: { sessionId?: string | null }) => {
  const summary = useAppStore((s) =>
    sessionId ? s.lastTurnSummary[sessionId] : undefined,
  )
  const usage = useAppStore((s) =>
    sessionId ? s.lastTurnUsage[sessionId] : undefined,
  )
  const totals = useAppStore((s) =>
    sessionId ? s.sessionTotals[sessionId] : undefined,
  )
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const { models } = useModels(true)

  const used = usage ? usage.input + (usage.cache_read ?? 0) : null
  if (used === null || !usage) return null
  const budget =
    models.find((m) => m.id === selectedModelId)?.contextWindow ??
    CONTEXT_BUDGET_TOKENS
  const fraction = used / budget
  const nearLimitClass =
    fraction > 0.95
      ? "text-red"
      : fraction > 0.8
        ? "text-yellow"
        : "text-ink-muted"

  return (
    <div className="group/usage relative">
      <button
        type="button"
        className={cn(
          "flex h-6 items-center gap-1 rounded-md px-1.5 text-sm transition-colors hover:text-ink-secondary",
          nearLimitClass,
        )}
        aria-label="Context usage"
      >
        <ContextRing fraction={fraction} />
        <span className="[font-variant-numeric:tabular-nums]">
          {Math.round(fraction * 100)}%
        </span>
      </button>

      <div
        role="tooltip"
        className={cn(
          "pointer-events-none absolute bottom-full right-0 z-50 mb-1.5 w-52",
          "rounded-lg border border-stroke-2 bg-panel p-2.5 text-sm shadow-[var(--shadow-md)]",
          "opacity-0 transition-opacity duration-[var(--duration-fast)]",
          "group-hover/usage:opacity-100 group-focus-within/usage:opacity-100",
        )}
      >
        <p className="mb-1.5 text-xs text-ink-faint">Last turn</p>
        <div className="flex flex-col gap-1">
          <UsageDetailRow label="Input" value={formatTokens(usage.input)} />
          <UsageDetailRow label="Output" value={formatTokens(usage.output)} />
          {usage.cache_read ? (
            <UsageDetailRow
              label="Cache read"
              value={formatTokens(usage.cache_read)}
            />
          ) : null}
          {usage.cache_write ? (
            <UsageDetailRow
              label="Cache write"
              value={formatTokens(usage.cache_write)}
            />
          ) : null}
          {usage.reasoning ? (
            <UsageDetailRow
              label="Reasoning"
              value={formatTokens(usage.reasoning)}
            />
          ) : null}
          <UsageDetailRow label="Budget" value={formatTokens(budget)} />
          {summary && typeof summary.cost_usd === "number" ? (
            <>
              <div className="my-0.5 border-t border-stroke-3" />
              <UsageDetailRow label="Cost" value={formatCost(summary.cost_usd)} />
            </>
          ) : null}
          {totals ? (
            <>
              <div className="my-0.5 border-t border-stroke-3" />
              <p className="text-xs text-ink-faint">Session total</p>
              <UsageDetailRow
                label="Tokens"
                value={formatTokens(totals.input + totals.output)}
              />
              {totals.costUsd > 0 ? (
                <UsageDetailRow label="Cost" value={formatCost(totals.costUsd)} />
              ) : null}
            </>
          ) : null}
        </div>
      </div>
    </div>
  )
}

/** Isolated-workspace badge → popover with the diff summary + integrate/discard. */
const IsolationBadge = ({
  sessionId,
  onError,
}: {
  sessionId: string
  onError?: (message: string) => void
}) => {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)

  const workspace = useWorkspaceActions(sessionId, onError, () =>
    setOpen(false),
  )

  const { data: status, isLoading } = useQuery({
    queryKey: ["workspace-status", sessionId],
    queryFn: () => workspaceStatus(sessionId),
    enabled: open,
    staleTime: 2_000,
  })

  useEffect(() => {
    const handlePointer = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener("mousedown", handlePointer)
    return () => document.removeEventListener("mousedown", handlePointer)
  }, [])

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={cn(
          "ml-1 rounded-full bg-fill-3 px-1.5 py-0.5 text-xs text-ink-muted",
          "transition-colors hover:bg-fill-2 hover:text-ink-secondary",
          open && "bg-fill-2 text-ink-secondary",
        )}
      >
        Isolated
      </button>

      {open ? (
        <div
          role="dialog"
          className={cn(
            "absolute bottom-full left-0 z-50 mb-1.5 w-64 overflow-hidden rounded-lg",
            "border border-stroke-2 bg-panel shadow-[var(--shadow-md)] animate-tray-in",
          )}
        >
          <div className="border-b border-stroke-3 px-3 py-2">
            <p className="text-sm font-medium text-ink-secondary">
              Isolated workspace
            </p>
            <p className="mt-0.5 text-xs text-ink-muted">
              {isLoading
                ? "Checking changes…"
                : status
                  ? `${status.filesChanged} file${status.filesChanged === 1 ? "" : "s"} changed${status.summary ? ` · ${status.summary}` : ""}`
                  : "No changes yet"}
            </p>
          </div>
          <button
            type="button"
            disabled={workspace.busy}
            onClick={() => void workspace.integrate()}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-base text-ink-secondary transition-colors hover:bg-fill-3 hover:text-ink disabled:pointer-events-none disabled:opacity-40"
          >
            <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Integrate into origin
          </button>
          <button
            type="button"
            disabled={workspace.busy}
            onClick={() => void workspace.discard()}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-base text-ink-secondary transition-colors hover:bg-fill-3 hover:text-ink disabled:pointer-events-none disabled:opacity-40"
          >
            <XCircle className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Discard workspace
          </button>
        </div>
      ) : null}
    </div>
  )
}

/** Right-aligned "N changes" pill + "Commit & Push" button, shown above the
 * composer for non-isolated sessions with a dirty working tree (design:
 * "Changes +9745 -737" pill + button). Clicking the pill jumps to the
 * Changes tab; the button opens an inline popover to compose the message. */
const CommitBar = ({
  sessionId,
  cwd,
  onError,
}: {
  sessionId: string
  cwd?: string
  onError?: (message: string) => void
}) => {
  const [open, setOpen] = useState(false)
  const [message, setMessage] = useState("Update from agent session")
  const [busy, setBusy] = useState<"commit" | "push" | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)

  const { data: files = [] } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId),
    enabled: !!cwd && !!sessionId,
    staleTime: 5_000,
  })

  const totals = files.reduce(
    (acc, f) => ({
      added: acc.added + (f.added ?? 0),
      removed: acc.removed + (f.removed ?? 0),
    }),
    { added: 0, removed: 0 },
  )

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ["git-status"] })
  }

  const handleCommit = async (andPush: boolean) => {
    if (busy) return
    setBusy("commit")
    try {
      // TODO: gitCommit stages the whole repo (`git add -A` in the Rust
      // `git_commit` command) even though the count/list above is
      // session-scoped (gitStatusSinceBaseline). A session with 0 tracked
      // changes can still commit unrelated pre-existing dirty files repo-wide.
      const sha = await gitCommit(sessionId, message.trim())
      invalidate()
      pushToast(`Committed ${sha}`, "success")
      if (andPush) {
        setBusy("push")
        try {
          await gitPush(sessionId)
          pushToast("Pushed", "success")
        } catch (err) {
          const msg = toInvokeError(err)
          pushToast(`Push failed: ${msg}`, "error")
          onError?.(msg)
        }
      }
      setOpen(false)
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Commit failed: ${msg}`, "error")
      onError?.(msg)
    } finally {
      setBusy(null)
    }
  }

  if (files.length === 0) return null

  return (
    <div ref={rootRef} className="relative flex shrink-0 items-center gap-1.5">
      <button
        type="button"
        onClick={() => {
          setRightPanelOpen(true)
          setRightPanelTab("changes")
        }}
        className={cn(
          "flex h-6 items-center gap-1 rounded-full bg-fill-3 px-2 text-xs text-ink-muted",
          "transition-colors hover:bg-fill-2 hover:text-ink-secondary",
        )}
      >
        {files.length} change{files.length === 1 ? "" : "s"}
        {totals.added > 0 ? (
          <span className="text-green">+{formatTokens(totals.added)}</span>
        ) : null}
        {totals.removed > 0 ? (
          <span className="text-red">-{formatTokens(totals.removed)}</span>
        ) : null}
      </button>

      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={cn(
          "flex h-6 items-center gap-1 rounded-md bg-accent px-2 text-xs text-accent-text",
          "transition-colors hover:bg-accent-hover",
        )}
      >
        <GitMerge className="h-3 w-3" aria-hidden />
        Commit & Push
      </button>

      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={rootRef}
        placement="above"
        role="dialog"
        aria-label="Commit changes"
        className="right-0 w-72 p-2.5"
      >
        <TextInput
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Commit message"
          aria-label="Commit message"
          autoFocus
        />
        <div className="mt-2 flex items-center justify-end gap-1.5">
          <Button
            variant="secondary"
            size="sm"
            isLoading={busy === "commit"}
            disabled={busy !== null || !message.trim()}
            onClick={() => void handleCommit(false)}
          >
            Commit
          </Button>
          <Button
            variant="primary"
            size="sm"
            isLoading={busy === "push"}
            disabled={busy !== null || !message.trim()}
            onClick={() => void handleCommit(true)}
          >
            Commit & Push
          </Button>
        </div>
      </PopoverTray>
    </div>
  )
}

type ContextBarProps = {
  cwd?: string
  sessionId?: string | null
  disabled?: boolean
  onError?: (message: string) => void
}

/** Context row above the composer: project · branch · isolation · context %. */
export const ContextBar = ({
  cwd,
  sessionId,
  disabled = false,
  onError,
}: ContextBarProps) => {
  const { data: isolated } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })

  // Gate the entire git cluster (branch pill + commit bar) on the cwd
  // actually being a git repo — a non-git folder should show none of it
  // rather than a misleading "No branch" pill. `isRepo` defaults to `true`
  // while the query is loading (or has no cwd yet) so the chrome doesn't
  // flash away/in on every session switch; it only ever hides once we
  // positively know there's no repo.
  const { data: isRepo = true } = useQuery({
    queryKey: ["git-is-repo", cwd],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 15_000,
  })

  return (
    <div className="flex min-h-[var(--status-bar-height)] items-center justify-between gap-2 px-1">
      <div className="flex min-w-0 items-center gap-0.5">
        <ProjectPicker
          sessionId={sessionId ?? null}
          cwd={cwd}
          disabled={disabled}
          onError={onError}
        />
        {isRepo ? (
          <BranchPicker cwd={cwd} disabled={disabled} onError={onError} />
        ) : null}
        {isolated && sessionId ? (
          <IsolationBadge sessionId={sessionId} onError={onError} />
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-2">
        {isRepo && !isolated && sessionId ? (
          <CommitBar sessionId={sessionId} cwd={cwd} onError={onError} />
        ) : null}
        <UsageRing sessionId={sessionId} />
      </div>
    </div>
  )
}
