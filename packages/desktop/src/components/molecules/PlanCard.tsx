import { Check, Circle, LoaderCircle } from "lucide-react"
import type { PlanEntry } from "../../lib/types"
import { cn } from "../../lib/utils"

type PlanCardProps = {
  entries: PlanEntry[]
}

export const PlanStatusIcon = ({ status }: { status: PlanEntry["status"] }) => {
  if (status === "completed") {
    return <Check className="h-3 w-3 text-green" aria-hidden />
  }
  if (status === "in_progress") {
    return (
      <LoaderCircle
        className="h-3 w-3 animate-spin text-accent"
        aria-hidden
      />
    )
  }
  return <Circle className="h-3 w-3 text-icon-3" aria-hidden />
}

/** plan checklist rendered from `plan_updated` events. */
export const PlanCard = ({ entries }: PlanCardProps) => {
  if (entries.length === 0) return null

  const done = entries.filter((e) => e.status === "completed").length

  return (
    <div
      className={cn(
        "overflow-hidden rounded-lg border border-stroke-3 bg-fill-5",
      )}
      role="list"
      aria-label={`Plan ${done} of ${entries.length} complete`}
    >
      <div className="flex items-center gap-2 border-b border-stroke-3 px-3 py-2">
        <span className="text-sm font-medium text-ink-secondary">Plan</span>
        <span className="text-sm text-ink-muted [font-variant-numeric:tabular-nums]">
          {done}/{entries.length}
        </span>
      </div>
      <ul className="flex flex-col gap-px px-1.5 py-1.5">
        {entries.map((entry, i) => (
          <li
            key={`${i}-${entry.content}`}
            role="listitem"
            className="flex items-start gap-2 rounded-md px-1.5 py-1.5"
          >
            <span className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center">
              <PlanStatusIcon status={entry.status} />
            </span>
            <span
              className={cn(
                "min-w-0 flex-1 text-base leading-relaxed",
                entry.status === "completed"
                  ? "text-ink-muted line-through"
                  : entry.status === "in_progress"
                    ? "text-ink"
                    : "text-ink-secondary",
              )}
            >
              {entry.content}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}
