import { MessageSquareText, X } from "lucide-react"
import type { PlanComment } from "../../stores/types"
import { formatRelativeTime, cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type PlanCommentListProps = {
  comments: PlanComment[]
  activeCommentId: string | null
  onFocus: (commentId: string) => void
  onRemove: (commentId: string) => void
  className?: string
}

/** Sidebar list of annotations on the open plan. */
export const PlanCommentList = ({
  comments,
  activeCommentId,
  onFocus,
  onRemove,
  className,
}: PlanCommentListProps) => {
  if (comments.length === 0) return null

  return (
    <div className={cn("mt-6", className)}>
      <h2 className="mb-1 flex items-center gap-1.5 text-sm font-medium text-ink-secondary">
        <MessageSquareText className="h-3.5 w-3.5" aria-hidden />
        Comments
      </h2>
      <ul>
        {comments.map((comment) => {
          const active = comment.id === activeCommentId
          return (
            <li
              key={comment.id}
              className={cn(
                "group flex items-start gap-2 border-b border-stroke-4 py-2 last:border-0",
                active && "bg-fill-2",
              )}
            >
              <Button
                variant="ghost"
                onClick={() => onFocus(comment.id)}
                className="h-auto min-w-0 flex-1 justify-start px-0 py-0 font-normal text-left"
              >
                <p className="line-clamp-2 text-xs italic text-ink-muted">
                  “{comment.quote}”
                </p>
                <p className="mt-0.5 text-sm text-ink">{comment.body}</p>
                <p className="mt-0.5 text-xs text-ink-faint">
                  {formatRelativeTime(comment.createdAtMs)}
                </p>
              </Button>
              <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Remove comment" title="Remove comment"
      onClick={() => onRemove(comment.id)}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6 opacity-0 transition-opacity group-hover:opacity-100",
      )}
    >
      <X className="h-3 w-3" aria-hidden />
    </Button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
