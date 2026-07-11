import {
  memo,
  useState,
  type KeyboardEvent,
} from "react"
import {
  ChevronRight,
  FilePenLine,
  FileSearch,
  LoaderCircle,
  Terminal,
  Wrench,
} from "lucide-react"
import type { ToolCall } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { DetailRow } from "./DetailRow"
import { DiffBadge } from "./ExecTail"
import {
  summarizeToolCalls,
  type ToolKind,
} from "../../lib/toolPresentation"

const KindIcon = ({
  kind,
  running,
}: {
  kind: ToolKind
  running: boolean
}) => {
  if (running) {
    return (
      <LoaderCircle
        className="h-3.5 w-3.5 shrink-0 animate-spin text-ink-faint"
        aria-hidden
      />
    )
  }
  if (kind === "explore") {
    return (
      <FileSearch className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  if (kind === "edit") {
    return (
      <FilePenLine className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  if (kind === "shell") {
    return (
      <Terminal className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  return <Wrench className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
}

type ToolStepGroupProps = {
  calls: ToolCall[]
  className?: string
  /** Keep expanded while any call in the group is still running. */
  forceOpen?: boolean
  /** Latest live progress note per call id (from `tool_progress`). */
  progress?: Record<string, string>
}

/** aggregated tool step: one summary line, expandable details. */
export const ToolStepGroup = memo(function ToolStepGroup({
  calls,
  className,
  forceOpen = false,
  progress,
}: ToolStepGroupProps) {
  const summary = summarizeToolCalls(calls)
  const [expanded, setExpanded] = useState(forceOpen || summary.running)
  const open = forceOpen || expanded
  const canExpand = summary.details.length > 0

  const handleToggle = () => {
    if (!canExpand) return
    setExpanded((v) => !v)
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault()
      handleToggle()
    }
    if (e.key === "Escape" && expanded) {
      e.preventDefault()
      setExpanded(false)
    }
  }

  return (
    <div
      className={cn(
        "group min-h-[var(--timeline-row-min-height)]",
        className,
      )}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-label={`${summary.title}, ${open ? "collapse" : "expand"} details`}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        className={cn(
          "flex w-full items-center gap-1 rounded-md py-px text-left text-base",
          "text-ink-muted transition-colors duration-[var(--duration-fast)]",
          "hover:text-ink-secondary focus-visible:outline-none",
          summary.failed && "text-danger",
        )}
      >
        <KindIcon kind={summary.kind} running={summary.running} />
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            summary.running && "animate-shimmer-text",
          )}
        >
          {summary.title}
        </span>
        <DiffBadge added={summary.added} removed={summary.removed} />
        {canExpand ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 shrink-0 text-icon-3",
              "transition-[transform,opacity] duration-[var(--duration-fast)]",
              open
                ? "rotate-90 opacity-100"
                : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </button>

      <Collapsible open={open}>
        <ul className="mt-0.5 ml-1.5 flex flex-col gap-0.5 py-0.5 pl-3">
          {summary.details.map((detail) => (
            <DetailRow
              key={detail.id}
              detail={detail}
              note={detail.running ? progress?.[detail.id] : undefined}
            />
          ))}
        </ul>
      </Collapsible>
    </div>
  )
})

/** Cluster consecutive same-kind tool rows for summaries. */
