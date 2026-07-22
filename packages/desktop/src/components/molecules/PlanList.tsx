import { Check, MessageSquareText } from "lucide-react"
import type { SessionPlan } from "../../stores/types"
import { formatRelativeTime, cn } from "../../lib/utils"
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item"
import { ScrollArea } from "@/components/ui/scroll-area"

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
      <div className="flex h-[var(--header-height)] shrink-0 items-center px-2.5">
        <h2 className="min-w-0 truncate text-sm font-medium text-ink">
          Review plans
          <span className="ml-2 font-normal text-ink-muted">
            {ordered.length} in this session
          </span>
        </h2>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="px-2.5 py-2">
        <ItemGroup className="gap-0.5">
          {ordered.map((plan) => {
            const commentCount = plan.comments.length
            return (
              <Item
                key={plan.id}
                render={<button type="button" onClick={() => onSelect(plan.id)} />}
                size="sm"
                className="cursor-pointer rounded-md border-transparent px-2.5 py-2 hover:bg-fill-4 focus-visible:bg-fill-4"
              >
                <ItemContent>
                  <ItemTitle className="gap-2 text-sm font-medium text-ink">
                    <span className="min-w-0 truncate">{plan.title}</span>
                    {plan.built ? (
                      <span className="inline-flex shrink-0 items-center gap-0.5 text-xs text-yellow">
                        <Check className="size-3" aria-hidden />
                        Built
                      </span>
                    ) : null}
                  </ItemTitle>
                  <ItemDescription className={cn("text-xs text-ink-faint")}>
                    <span className="inline-flex items-center gap-2">
                      <span>{formatRelativeTime(plan.createdAtMs)}</span>
                      {commentCount > 0 ? (
                        <span className="inline-flex items-center gap-0.5">
                          <MessageSquareText className="size-3" aria-hidden />
                          {commentCount}
                        </span>
                      ) : null}
                    </span>
                  </ItemDescription>
                </ItemContent>
              </Item>
            )
          })}
        </ItemGroup>
        </div>
      </ScrollArea>
    </div>
  )
}
