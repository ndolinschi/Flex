import { MessageSquareText } from "lucide-react"
import { Button } from "../atoms"
import { cn } from "../../lib/utils"
import type { PlanSelectionAnchor } from "../../hooks/usePlanSelectionComment"

type PlanCommentButtonProps = {
  selection: PlanSelectionAnchor | null
  onComment: () => void
  className?: string
}

/** Floating "Comment" control anchored to the current plan-text selection. */
export const PlanCommentButton = ({
  selection,
  onComment,
  className,
}: PlanCommentButtonProps) => {
  if (!selection) return null

  const left = Math.min(
    Math.max(8, selection.anchor.x - 44),
    window.innerWidth - 100,
  )
  const top = Math.min(
    Math.max(8, selection.anchor.y),
    window.innerHeight - 40,
  )

  return (
    <div
      data-suppress-native-webview=""
      className={cn("fixed z-50", className)}
      style={{ left, top }}
    >
      <Button
        variant="default"
        size="sm"
        onMouseDown={(e) => {
          // Keep the selection; a click would otherwise collapse it.
          e.preventDefault()
        }}
        onClick={onComment}
        aria-label="Comment on selection"
      >
        <MessageSquareText className="h-3.5 w-3.5" aria-hidden />
        Comment
      </Button>
    </div>
  )
}
