import { Check, MessageSquareText } from "lucide-react"
import type { SessionPlan } from "../../stores/types"
import { formatRelativeTime, cn } from "../../lib/utils"

type PlanListProps = {
  plans: SessionPlan[]
  onSelect: (planId: string) => void
  className?: string
}

/** Multi-plan "Review plans" list — shown when a session has more than one
 * ExitPlanMode document. Newest plans are listed first. */
export const PlanList = ({ plans, onSelect, className }: PlanListProps) => {
  const ordered = [...plans].sort((a, b) => b.createdAtMs - a.createdAtMs)

  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>
      <div className="flex h-[var(--header-height)] shrink-0 items-center px-4">
        <h2 className="min-w-0 truncate text-sm font-medium text-ink">
          Review plans
          <span className="ml-2 font-normal text-ink-muted">
            {ordered.length} in this session
          </span>
        </h2>
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto px-2 py-2" role="list">
        {ordered.map((plan) => {
          const commentCount = plan.comments.length
          return (
            <li key={plan.id}>
              <button
                type="button"
                onClick={() => onSelect(plan.id)}
                className={cn(
                  "flex w-full items-start gap-3 rounded-md px-2.5 py-2.5 text-left",
                  "transition-colors duration-[var(--duration-fast)] hover:bg-fill-3",
                )}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 truncate text-sm font-medium text-ink">
                      {plan.title}
                    </span>
                    {plan.built ? (
                      <span className="inline-flex shrink-0 items-center gap-0.5 text-xs text-yellow">
                        <Check className="h-3 w-3" aria-hidden />
                        Built
                      </span>
                    ) : null}
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-faint">
                    <span>{formatRelativeTime(plan.createdAtMs)}</span>
                    {commentCount > 0 ? (
                      <span className="inline-flex items-center gap-0.5">
                        <MessageSquareText className="h-3 w-3" aria-hidden />
                        {commentCount}
                      </span>
                    ) : null}
                  </div>
                </div>
              </button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
