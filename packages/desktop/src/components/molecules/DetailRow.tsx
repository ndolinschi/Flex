import { useEffect, useState, type MouseEvent } from "react"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { ChevronRight, FileCode2, ListEnd, LoaderCircle } from "lucide-react"
import { backgroundDemote, reviewFileDiff, toInvokeError } from "../../lib/tauri"
import { cn, toSessionRelativePath } from "../../lib/utils"
import { sessionScopeKey, useAppStore } from "../../stores/appStore"
import { useSessions } from "../../hooks/useSessions"
import { Collapsible } from "./Collapsible"
import { ChatDiffCard } from "./ChatDiffCard"
import { BackgroundBashRow } from "./BackgroundBashRow"
import { DiffBadge, ExecErrorAction, ExecTail } from "./ExecTail"
import type { ToolStepDetail } from "../../lib/toolPresentation"

const DemoteButton = ({ callId }: { callId: string }) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const [demoting, setDemoting] = useState(false)

  const handleDemote = () => {
    if (!sessionId || demoting) return
    setDemoting(true)
    backgroundDemote(sessionId, callId).finally(() => setDemoting(false))
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Move to background" title="Move to background"
      onClick={handleDemote}
      disabled={demoting}
      className={cn(
        "text-ink-secondary hover:bg-fill-4 hover:text-ink",
        "ml-1 h-5 w-5 shrink-0",
      )}
    >
      {demoting ? <Spinner /> : (
        <ListEnd className="h-3 w-3" aria-hidden />
      )}
    </Button>
  )
}

type DetailRowProps = {
  detail: ToolStepDetail
  note?: string
  /** When true, expand and fetch the file diff on mount (single Edit/Write groups). */
  autoExpandDiff?: boolean
}

/** Single detail line under a tool-step group. Edit/write rows that carry a
 * resolvable `diffPath` become expandable: first expand lazy-fetches the
 * file's diff against its pre-agent base state and renders it inline
 * (display-only chat card — no hunk actions; Changes tab owns Keep/Undo).
 * Rows without a path behave exactly as before. */
export const DetailRow = ({
  detail,
  note,
  autoExpandDiff = false,
}: DetailRowProps) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const openWorkspaceFile = useAppStore((s) => s.openWorkspaceFile)
  const { sessions } = useSessions()
  const cwd = sessions.find((s) => s.id === sessionId)?.cwd
  const [expanded, setExpanded] = useState(autoExpandDiff)
  const [diff, setDiff] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const rawPath = detail.diffPath ?? detail.filePath
  const relativePath = rawPath
    ? toSessionRelativePath(rawPath, cwd)
    : undefined

  // Auto-expand single Edit/Write: fetch once when the row can resolve a path.
  useEffect(() => {
    if (!autoExpandDiff) return
    if (!detail.diffPath || !sessionId || !relativePath) return
    if (detail.background) return
    setExpanded(true)
    setLoading(true)
    setError(null)
    let cancelled = false
    reviewFileDiff(sessionId, relativePath)
      .then((text) => {
        if (!cancelled) setDiff(text)
      })
      .catch((err) => {
        if (!cancelled) setError(toInvokeError(err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [
    autoExpandDiff,
    detail.diffPath,
    detail.background,
    sessionId,
    relativePath,
  ])

  if (detail.background) {
    return <BackgroundBashRow detail={detail} />
  }

  const canExpand = !!detail.diffPath && !!sessionId && !!relativePath
  const canOpenFile =
    !!sessionId && !!relativePath && !relativePath.endsWith("/")

  const loadDiff = () => {
    if (!canExpand || !relativePath || !sessionId) return
    if (diff !== null || loading) return
    setLoading(true)
    setError(null)
    reviewFileDiff(sessionId, relativePath)
      .then((text) => setDiff(text))
      .catch((err) => setError(toInvokeError(err)))
      .finally(() => setLoading(false))
  }

  const handleToggle = () => {
    if (!canExpand || !relativePath) return
    const next = !expanded
    setExpanded(next)
    if (next) loadDiff()
  }

  const handleOpenFile = (e: MouseEvent) => {
    e.stopPropagation()
    if (!sessionId || !relativePath) return
    openWorkspaceFile(sessionScopeKey(sessionId), relativePath)
  }

  return (
    <li
      className={cn(
        "flex flex-col animate-tool-step-in",
        detail.failed && "text-destructive",
        detail.running && "text-ink-faint",
      )}
    >
      <div
        role={canExpand ? "button" : undefined}
        tabIndex={canExpand ? 0 : undefined}
        aria-expanded={canExpand ? expanded : undefined}
        onClick={canExpand ? handleToggle : undefined}
        onKeyDown={
          canExpand
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault()
                  handleToggle()
                }
              }
            : undefined
        }
        className={cn(
          // Cursor tool detail: secondary label + tertiary meta, gap 4.
          "group/detail flex min-h-6 items-center gap-1 text-base leading-[1.5] text-ink-muted",
          "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          canExpand && "cursor-pointer hover:text-ink-secondary",
        )}
      >
        {/* Fixed-size leading slot — running→done swaps the spinner for a
         * chevron (or nothing, when not expandable) in place, so the box
         * itself never changes size and the row never shifts. */}
        <span className="flex h-[18px] w-4 shrink-0 items-center justify-center">
          {detail.running ? (
            <LoaderCircle className="h-3 w-3 animate-spin" aria-hidden />
          ) : canExpand ? (
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 text-icon-3 transition-transform duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                expanded && "rotate-90",
              )}
              aria-hidden
            />
          ) : null}
        </span>
        <span className="min-w-0 shrink truncate text-base [font-variant-numeric:tabular-nums] text-ink-secondary">
          {detail.label}
        </span>
        {note ? (
          <span className="min-w-0 shrink truncate text-xs text-ink-faint">
            {note}
          </span>
        ) : detail.sublabel ? (
          <span className="shrink-0 text-xs text-ink-faint">{detail.sublabel}</span>
        ) : null}
        <DiffBadge added={detail.added} removed={detail.removed} />
        {canOpenFile ? (
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Open file" title="Open file"
      onClick={handleOpenFile}
      className={cn(
        "text-ink-secondary hover:bg-fill-4 hover:text-ink",
        "ml-auto h-5 w-5 shrink-0 opacity-0 group-hover/detail:opacity-100",
      )}
    >
      <FileCode2 className="h-3 w-3" aria-hidden />
    </Button>
        ) : null}
        {detail.canDemote ? <DemoteButton callId={detail.id} /> : null}
      </div>
      {detail.isShell ? (
        <ExecTail callId={detail.id} muted={!detail.running} />
      ) : null}
      {detail.isShell && !detail.running ? (
        <ExecErrorAction callId={detail.id} command={detail.command ?? detail.label} />
      ) : null}
      {canExpand ? (
        <Collapsible open={expanded}>
          <div className="ml-3.5 mt-0.5">
            {loading ? (
              // Match ChatDiffCard shell (r-lg, pad 10×6, hairline) so expand
              // states don't jump chrome density.
              <div className="rounded-[var(--radius-lg)] border border-stroke-3 bg-panel px-2.5 py-1.5 text-xs leading-[1.5] text-ink-faint">
                Loading diff…
              </div>
            ) : error ? (
              <div className="rounded-[var(--radius-lg)] border border-stroke-3 bg-panel px-2.5 py-1.5 text-xs leading-[1.5] text-destructive">
                Diff unavailable — {error}
              </div>
            ) : diff ? (
              <ChatDiffCard
                diff={diff}
                path={relativePath}
                maxHeight={300}
                onOpenFile={
                  canOpenFile
                    ? () => {
                        if (!sessionId || !relativePath) return
                        openWorkspaceFile(
                          sessionScopeKey(sessionId),
                          relativePath,
                        )
                      }
                    : undefined
                }
              />
            ) : (
              <div className="rounded-[var(--radius-lg)] border border-stroke-3 bg-panel px-2.5 py-1.5 text-xs leading-[1.5] text-ink-faint">
                No changes vs HEAD
              </div>
            )}
          </div>
        </Collapsible>
      ) : null}
    </li>
  )
}
