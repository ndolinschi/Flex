import { useEffect, useState } from "react"
import { Bot, ChevronDown, Network } from "lucide-react"
import type { SubagentTimelineRow } from "../../lib/workerPresentation"
import {
  summarizeWorkerActivity,
  workerTitle,
} from "../../lib/workerPresentation"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "../../lib/utils"

type WorkingAgentsPillProps = {
  workers: SubagentTimelineRow[]
  /** Scroll the timeline to the active workers group. */
  onScrollToWorkers?: () => void
}

/** Composer-adjacent glance: "N Working" opens a short menu of running
 * worker titles (open viewer / scroll to group). Hidden when none running.
 * Menu body (per-worker summarize) only mounts while open. */
export const WorkingAgentsPill = ({
  workers,
  onScrollToWorkers,
}: WorkingAgentsPillProps) => {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const [open, setOpen] = useState(false)

  useEffect(() => {
    if (workers.length === 0) setOpen(false)
  }, [workers.length])

  if (workers.length === 0) return null

  const n = workers.length
  const label = n === 1 ? "1 Working" : `${n} Working`

  return (
    <div className="relative mb-1.5 flex justify-start">
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger
          render={
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label="Running workers"
              className={cn(
                "rounded-md border border-border bg-muted px-2 text-muted-foreground",
                "hover:bg-muted/80 hover:text-foreground",
              )}
            />
          }
        >
          <Network className="size-3 shrink-0 text-muted-foreground" aria-hidden />
          <span className="animate-shimmer-text">{label}</span>
          <ChevronDown
            className={cn(
              "size-2.5 text-muted-foreground transition-transform duration-[var(--duration-fast)]",
              open && "rotate-180",
            )}
            aria-hidden
          />
        </DropdownMenuTrigger>
        {open ? (
          <DropdownMenuContent
            align="start"
            side="top"
            sideOffset={6}
            className="min-w-[220px] max-w-[320px]"
          >
            <DropdownMenuGroup>
              {onScrollToWorkers ? (
                <>
                  <DropdownMenuItem
                    onClick={() => {
                      setOpen(false)
                      onScrollToWorkers()
                    }}
                  >
                    <Network />
                    Jump to workers
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                </>
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
                    className="h-auto items-start py-1.5"
                    onClick={() => {
                      setOpen(false)
                      openSubagentViewer(w.childSession, title)
                    }}
                  >
                    <Bot className="mt-0.5" />
                    <span className="min-w-0 flex-1 text-left">
                      <span className="block truncate text-sm text-foreground">
                        {title}
                      </span>
                      {activity.latestLabel ? (
                        <span className="block truncate text-xs text-muted-foreground">
                          {activity.latestLabel}
                        </span>
                      ) : null}
                    </span>
                  </DropdownMenuItem>
                )
              })}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        ) : null}
      </DropdownMenu>
    </div>
  )
}
