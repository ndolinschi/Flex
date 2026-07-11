import { useState } from "react"
import { ChevronRight, ListEnd, LoaderCircle } from "lucide-react"
import { backgroundDemote, reviewFileDiff } from "../../lib/tauri"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { Collapsible } from "./Collapsible"
import { DiffView } from "./DiffView"
import { IconButton } from "../atoms/IconButton"
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
    <IconButton
      label="Move to background"
      isLoading={demoting}
      onClick={handleDemote}
      className="ml-1 h-5 w-5 shrink-0"
    >
      <ListEnd className="h-3 w-3" aria-hidden />
    </IconButton>
  )
}

type DetailRowProps = {
  detail: ToolStepDetail
  note?: string
}

/** Single detail line under a tool-step group. Edit/write rows that carry a
 * resolvable `diffPath` become expandable: first expand lazy-fetches the
 * file's diff against its pre-agent base state and renders it inline
 * (display-only — no hunk actions, this is a timeline row, not the Changes
 * tab). Rows without a path behave exactly as before. */
export const DetailRow = ({ detail, note }: DetailRowProps) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const [expanded, setExpanded] = useState(false)
  const [diff, setDiff] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)

  if (detail.background) {
    return <BackgroundBashRow detail={detail} />
  }

  const canExpand = !!detail.diffPath && !!sessionId

  const handleToggle = () => {
    if (!canExpand) return
    const next = !expanded
    setExpanded(next)
    if (next && diff === null && !loading) {
      setLoading(true)
      setError(false)
      reviewFileDiff(sessionId!, detail.diffPath!)
        .then((text) => setDiff(text))
        .catch(() => setError(true))
        .finally(() => setLoading(false))
    }
  }

  return (
    <li
      className={cn(
        "flex flex-col",
        detail.failed && "text-danger",
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
          "flex min-h-6 items-center gap-1 text-[13px] leading-[1.5] text-ink-muted",
          canExpand && "cursor-pointer",
        )}
      >
        {/* Fixed-size leading slot — running→done swaps the spinner for a
         * chevron (or nothing, when not expandable) in place, so the box
         * itself never changes size and the row never shifts. */}
        <span className="flex h-3 w-3 shrink-0 items-center justify-center">
          {detail.running ? (
            <LoaderCircle className="h-3 w-3 animate-spin" aria-hidden />
          ) : canExpand ? (
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 text-icon-3 transition-transform duration-[var(--duration-fast)]",
                expanded && "rotate-90",
              )}
              aria-hidden
            />
          ) : null}
        </span>
        <span className="min-w-0 shrink truncate text-[12px] [font-variant-numeric:tabular-nums] text-ink-secondary">
          {detail.label}
        </span>
        {note ? (
          <span className="min-w-0 shrink truncate text-ink-faint">
            {note}
          </span>
        ) : detail.sublabel ? (
          <span className="shrink-0 text-ink-faint">{detail.sublabel}</span>
        ) : null}
        <DiffBadge added={detail.added} removed={detail.removed} />
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
          <div className="ml-3.5 max-h-[300px] overflow-auto rounded-md border border-stroke-3 bg-panel py-1">
            {loading ? (
              <div className="px-3 py-1 text-[12px] text-ink-faint">
                Loading diff…
              </div>
            ) : error ? (
              <div className="px-3 py-1 text-[12px] text-ink-faint">
                Diff unavailable — file may be outside this session's workspace
              </div>
            ) : diff ? (
              <DiffView diff={diff} />
            ) : null}
          </div>
        </Collapsible>
      ) : null}
    </li>
  )
}

