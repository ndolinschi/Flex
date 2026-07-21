import {
  memo,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react"
import {
  ChevronRight,
  FilePenLine,
  FileSearch,
  ListChecks,
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
import { Button } from "@/components/ui/button"

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
  if (kind === "plan") {
    return (
      <ListChecks className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
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
  const singleEditDiff =
    summary.kind === "edit" &&
    !summary.running &&
    summary.details.length === 1 &&
    !!summary.details[0]?.diffPath
  const [expanded, setExpanded] = useState(
    forceOpen || summary.running || singleEditDiff,
  )
  const open = forceOpen || expanded
  const canExpand = summary.details.length > 0

  // Auto-expand while `forceOpen` (cluster still running). When the run
  // finishes, `forceOpen` drops — keep open for a single Edit/Write so the
  // chat diff card stays visible; otherwise collapse. Only reacts to
  // `forceOpen` / single-edit settling so a manual toggle isn't clobbered.
  const prevForceOpen = useRef(forceOpen)
  const prevSingleEdit = useRef(singleEditDiff)
  useEffect(() => {
    if (prevForceOpen.current !== forceOpen) {
      if (forceOpen) {
        setExpanded(true)
      } else if (singleEditDiff) {
        setExpanded(true)
      } else {
        setExpanded(false)
      }
      prevForceOpen.current = forceOpen
    } else if (prevSingleEdit.current !== singleEditDiff && singleEditDiff) {
      setExpanded(true)
    }
    prevSingleEdit.current = singleEditDiff
  }, [forceOpen, singleEditDiff])

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
        "group min-h-[var(--timeline-row-min-height)] animate-tool-step-in",
        className,
      )}
    >
      <Button
        variant="ghost"
        aria-expanded={open}
        aria-label={`${summary.title}, ${open ? "collapse" : "expand"} details`}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        className={cn(
          "h-auto w-full justify-start gap-1 rounded-md px-0 py-px font-normal text-base",
          "text-ink-muted hover:bg-transparent hover:text-ink-secondary aria-expanded:bg-transparent",
          summary.failed && "text-destructive",
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
      </Button>

      <Collapsible open={open}>
        {/* Single `pt-0.5` under the summary — avoid stacking mt + py which
            left an uneven air gap above the first detail (RepoMap QA). */}
        <ul className="ml-1.5 flex flex-col gap-0.5 pt-0.5 pl-3">
          {summary.details.map((detail) => (
            <DetailRow
              key={detail.id}
              detail={detail}
              note={detail.running ? progress?.[detail.id] : undefined}
              autoExpandDiff={singleEditDiff && detail.id === summary.details[0]?.id}
            />
          ))}
        </ul>
      </Collapsible>
    </div>
  )
})

/** Cluster consecutive same-kind tool rows for summaries. */
