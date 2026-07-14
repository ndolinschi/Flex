import { memo, useState, type ReactNode } from "react"
import {
  Bot,
  Check,
  ChevronRight,
  LoaderCircle,
  PanelBottomOpen,
  X,
} from "lucide-react"
import type { TimelineRow, TurnSummary } from "../../lib/types"
import {
  filterWorkerDisplayChildren,
  summarizeWorkerActivity,
  type WorkerStatus,
} from "../../lib/workerPresentation"
import { cn, formatDuration } from "../../lib/utils"
import { Collapsible } from "./Collapsible"

type SubagentGroupProps = {
  task: string
  role?: string
  phase: "started" | "completed"
  durationMs?: number
  summary?: TurnSummary
  /** Opens the bottom subagent viewer with this child's live feed. */
  onOpenViewer?: () => void
  /**
   * Compact worker row (inside WorkersGroup): status + activity + meta,
   * no nested tool dump by default. Expand still peeks recent tools.
   */
  compact?: boolean
  /** Raw nested timeline for activity / tool-count (preferred). */
  nestedRows?: TimelineRow[]
  children?: ReactNode
}

/** First line of a (possibly multi-line) task prompt. */
const firstLine = (text: string): string => text.split("\n", 1)[0]

const StatusGlyph = ({ status }: { status: WorkerStatus }) => {
  if (status === "running") {
    return (
      <LoaderCircle
        className="h-3.5 w-3.5 shrink-0 animate-spin text-ink-faint"
        aria-hidden
      />
    )
  }
  if (status === "failed") {
    return <X className="h-3.5 w-3.5 shrink-0 text-danger" aria-hidden />
  }
  return <Check className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
}

/** Small expandable "Task prompt" row. */
const TaskPromptDetail = ({ task }: { task: string }) => {
  const [expanded, setExpanded] = useState(false)
  const preview = firstLine(task)

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        className="group/prompt flex min-h-5 w-full items-center gap-1 text-left text-[13px] leading-[1.5] text-ink-muted"
      >
        <ChevronRight
          className={cn(
            "h-2.5 w-2.5 shrink-0 text-icon-3 transition-transform duration-[var(--duration-fast)]",
            expanded && "rotate-90",
          )}
          aria-hidden
        />
        <span className="shrink-0 text-ink-faint">Task prompt</span>
        <span className="min-w-0 flex-1 truncate text-ink-faint">
          {preview}
        </span>
      </button>
      <Collapsible open={expanded}>
        <div className="ml-3.5 max-h-[300px] overflow-auto rounded-md border border-stroke-3 bg-panel px-3 py-2">
          <p className="whitespace-pre-wrap text-[13px] leading-[1.5] text-ink-muted">
            {task}
          </p>
        </div>
      </Collapsible>
    </div>
  )
}

/** Collapsible nested subagent work block — enriched with live status,
 * latest activity, and tool-count meta. */
export const SubagentGroup = memo(function SubagentGroup({
  task,
  role,
  phase,
  durationMs,
  summary,
  onOpenViewer,
  compact = false,
  nestedRows,
  children,
}: SubagentGroupProps) {
  const activity = nestedRows
    ? summarizeWorkerActivity(nestedRows, phase, summary)
    : {
        status: (phase === "started"
          ? "running"
          : summary?.stop_reason === "error" ||
              summary?.stop_reason === "max_iterations"
            ? "failed"
            : "completed") as WorkerStatus,
        latestLabel: null as string | null,
        toolCount: 0,
        hasError: false,
      }
  const status = activity.status

  const [expanded, setExpanded] = useState(!compact && phase === "started")
  const open = compact ? expanded : phase === "started" || expanded

  const title = `${role ? `${role} — ` : ""}${firstLine(task)}`
  const metaParts: string[] = []
  if (activity.toolCount > 0) {
    metaParts.push(
      `${activity.toolCount} tool${activity.toolCount === 1 ? "" : "s"}`,
    )
  }
  if (phase === "completed" && typeof durationMs === "number") {
    metaParts.push(formatDuration(durationMs))
  }
  const activityLine =
    status === "running"
      ? activity.latestLabel ?? "Working…"
      : status === "failed"
        ? activity.latestLabel ?? "Failed"
        : activity.latestLabel

  const peekRows = nestedRows
    ? filterWorkerDisplayChildren(nestedRows)
        .filter((r) => r.type === "tool" || r.type === "assistant")
        .slice(-6)
    : []

  const hasBody =
    !!children || peekRows.length > 0 || (compact && !!nestedRows)

  return (
    <div className="flex flex-col">
      <div className="group flex min-h-7 w-full items-start gap-1.5">
        <button
          type="button"
          onClick={() => {
            if (onOpenViewer) {
              onOpenViewer()
              return
            }
            if (phase === "started" && !compact) return
            setExpanded((v) => !v)
          }}
          className={cn(
            "flex min-w-0 flex-1 flex-col gap-0.5 text-left",
            (onOpenViewer || phase !== "started" || compact) && "cursor-pointer",
          )}
        >
          <span className="flex min-w-0 items-center gap-1.5 text-base">
            {compact ? (
              <StatusGlyph status={status} />
            ) : (
              <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
            )}
            <span
              className={cn(
                "min-w-0 truncate text-ink-secondary",
                status === "running" && "animate-shimmer-text",
                status === "failed" && "text-danger",
              )}
            >
              {title}
            </span>
            {metaParts.length > 0 ? (
              <span className="shrink-0 text-[13px] text-ink-faint [font-variant-numeric:tabular-nums]">
                {metaParts.join(" · ")}
              </span>
            ) : null}
            {onOpenViewer ? (
              <PanelBottomOpen
                className={cn(
                  "h-3 w-3 shrink-0 text-icon-3 opacity-0",
                  "transition-opacity duration-[var(--duration-fast)] group-hover:opacity-100",
                )}
                aria-hidden
              />
            ) : null}
          </span>
          {activityLine ? (
            <span
              className={cn(
                "min-w-0 truncate pl-5 text-[13px] leading-[1.4] text-ink-faint",
                status === "running" && "animate-shimmer-text",
              )}
            >
              {activityLine}
            </span>
          ) : null}
        </button>
        {(phase === "completed" || compact) && hasBody ? (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            aria-expanded={open}
            aria-label="Toggle inline details"
            className="mt-1 shrink-0 cursor-pointer p-0.5"
          >
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
                "group-hover:opacity-100",
                open && "rotate-90 opacity-100",
              )}
              aria-hidden
            />
          </button>
        ) : null}
      </div>
      <Collapsible open={open && hasBody}>
        <div className="ml-1.5 flex flex-col gap-1 py-1 pl-3">
          <TaskPromptDetail task={task} />
          {children
            ? children
            : peekRows.map((row) => {
                if (row.type === "tool") {
                  const label =
                    summarizeWorkerActivity([row], "completed").latestLabel ??
                    row.call.tool_name
                  return (
                    <p
                      key={row.id}
                      className="truncate text-[13px] leading-[1.5] text-ink-faint"
                    >
                      {label}
                    </p>
                  )
                }
                if (row.type === "assistant" && row.text.trim()) {
                  return (
                    <p
                      key={row.id}
                      className="line-clamp-2 text-[13px] leading-[1.5] text-ink-muted"
                    >
                      {row.text.trim()}
                    </p>
                  )
                }
                return null
              })}
        </div>
      </Collapsible>
    </div>
  )
})
