import { memo, useState, type ReactNode } from "react"
import { ChevronRight, Bot, PanelBottomOpen } from "lucide-react"
import { cn, formatDuration } from "../../lib/utils"
import { Collapsible } from "./Collapsible"

type SubagentGroupProps = {
  task: string
  role?: string
  phase: "started" | "completed"
  durationMs?: number
  /** Opens the bottom subagent viewer with this child's live feed. */
  onOpenViewer?: () => void
  children: ReactNode
}

/** First line of a (possibly multi-line) task prompt, for the collapsed
 * preview — full text is only ever shown after an explicit click. */
const firstLine = (text: string): string => text.split("\n", 1)[0]

/** Small expandable "Task prompt" row: a subagent's full task prompt used to
 * render inline as a giant message-row block wherever `SubagentGroup`'s
 * children were spread out — collapsed to a one-line truncated preview here,
 * with the full text only a click away in a scrollable block. */
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

/** Collapsible nested subagent work block . */
export const SubagentGroup = memo(function SubagentGroup({
  task,
  role,
  phase,
  durationMs,
  onOpenViewer,
  children,
}: SubagentGroupProps) {
  const [expanded, setExpanded] = useState(phase === "started")
  const open = phase === "started" || expanded

  return (
    <div className="flex flex-col pl-1">
      <div className="group flex min-h-7 w-full items-center gap-1.5">
        <button
          type="button"
          onClick={() => {
            if (onOpenViewer) {
              onOpenViewer()
              return
            }
            if (phase === "started") return
            setExpanded((v) => !v)
          }}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-1.5 text-left text-base",
            (onOpenViewer || phase !== "started") && "cursor-pointer",
          )}
        >
          <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
          <span className="min-w-0 truncate text-ink-secondary">
            {role ? `${role} — ` : ""}
            {task}
          </span>
          {phase === "completed" && typeof durationMs === "number" ? (
            <span className="shrink-0 text-ink-muted [font-variant-numeric:tabular-nums]">
              {formatDuration(durationMs)}
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
        </button>
        {phase === "completed" ? (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            aria-expanded={open}
            aria-label="Toggle inline details"
            className="shrink-0 cursor-pointer p-0.5"
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
      <Collapsible open={open && !!children}>
        <div className="ml-1.5 flex flex-col gap-1 py-1 pl-3">
          <TaskPromptDetail task={task} />
          {children}
        </div>
      </Collapsible>
    </div>
  )
})
