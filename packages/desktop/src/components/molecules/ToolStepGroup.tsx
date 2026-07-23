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
        className="size-3.5 shrink-0 animate-spin text-icon-3"
        strokeWidth={1.5}
        aria-hidden
      />
    )
  }
  const iconClass = "size-3.5 shrink-0 text-icon-3"
  if (kind === "explore") {
    return <FileSearch className={iconClass} strokeWidth={1.5} aria-hidden />
  }
  if (kind === "edit") {
    return <FilePenLine className={iconClass} strokeWidth={1.5} aria-hidden />
  }
  if (kind === "shell") {
    return <Terminal className={iconClass} strokeWidth={1.5} aria-hidden />
  }
  if (kind === "plan") {
    return <ListChecks className={iconClass} strokeWidth={1.5} aria-hidden />
  }
  return <Wrench className={iconClass} strokeWidth={1.5} aria-hidden />
}

type ToolStepGroupProps = {
  calls: ToolCall[]
  className?: string
  forceOpen?: boolean
  progress?: Record<string, string>
}

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
          "h-auto w-full justify-start gap-1 rounded-sm px-0 py-px font-normal text-base leading-[1.5]",
          "text-ink-muted hover:bg-transparent hover:text-ink-secondary aria-expanded:bg-transparent",
          summary.failed && "text-destructive",
        )}
      >
        <span className="flex h-[18px] w-4 shrink-0 items-center justify-center">
          <KindIcon kind={summary.kind} running={summary.running} />
        </span>
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-ink-secondary",
            summary.running && "animate-shimmer-text",
            summary.failed && "text-danger",
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

