import { useEffect, useMemo, useState } from "react"
import { Bot, ChevronDown, Network } from "@/components/icons"
import type { TimelineRow } from "../../lib/types"
import {
  collectRunningWorkers,
  summarizeWorkerActivity,
  workerTitle,
} from "../../lib/workerPresentation"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type WorkingAgentsPillProps = {
  rows: TimelineRow[]
  /** Scroll the timeline to the active workers group. */
  onScrollToWorkers?: () => void
}

/** Composer-adjacent glance: "N Working" opens a short menu of running
 * worker titles (open viewer / scroll to group). Hidden when none running. */
export const WorkingAgentsPill = ({
  rows,
  onScrollToWorkers,
}: WorkingAgentsPillProps) => {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const workers = useMemo(() => collectRunningWorkers(rows), [rows])
  const [open, setOpen] = useState(false)

  useEffect(() => {
    if (workers.length === 0) setOpen(false)
  }, [workers.length])

  if (workers.length === 0) return null

  const n = workers.length
  const label = n === 1 ? "1 Working" : `${n} Working`

  return (
    <div className="mb-1.5 flex justify-start">
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-haspopup="menu"
            className={cn(
              "inline-flex h-6 items-center gap-1.5 rounded-md border border-stroke-3",
              "bg-fill-3 px-2 text-sm text-ink-secondary",
              "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink",
            )}
          >
            <Network className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
            <span className="animate-shimmer-text">{label}</span>
            <ChevronDown
              className={cn(
                "h-2.5 w-2.5 text-icon-3 transition-transform duration-[var(--duration-fast)]",
                open && "rotate-180",
              )}
              aria-hidden
            />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          side="top"
          align="start"
          sideOffset={6}
          className="min-w-[220px] max-w-[320px] rounded-md border border-stroke-3 bg-panel p-1 shadow-[var(--shadow-popover)]"
        >
          <DropdownMenuGroup>
            {onScrollToWorkers ? (
              <DropdownMenuItem
                className="gap-2 px-2 py-1.5"
                onSelect={() => onScrollToWorkers()}
              >
                <Network className="size-3.5 text-ink-faint" aria-hidden />
                Jump to workers
              </DropdownMenuItem>
            ) : null}
            {workers.map((w) => {
              const activity = summarizeWorkerActivity(
                w.children,
                w.phase,
                w.summary,
              )
              const title = workerTitle(w.role, w.task)
              return (
                <DropdownMenuItem
                  key={w.childSession}
                  className="flex-col items-start gap-0.5 px-2 py-1.5"
                  onSelect={() => openSubagentViewer(w.childSession, title)}
                >
                  <span className="flex min-w-0 items-center gap-1.5 text-sm text-ink-secondary">
                    <Bot className="size-3.5 shrink-0 text-ink-faint" aria-hidden />
                    <span className="min-w-0 truncate">{title}</span>
                  </span>
                  {activity.latestLabel ? (
                    <span className="truncate pl-5 text-sm text-ink-faint">
                      {activity.latestLabel}
                    </span>
                  ) : null}
                </DropdownMenuItem>
              )
            })}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
