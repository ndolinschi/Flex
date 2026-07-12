import { RotateCw, X } from "lucide-react"
import { Button, IconButton } from "../atoms"
import { cn } from "../../lib/utils"

type SidebarResumeErrorProps = {
  message: string
  onRetry: () => void
  onDismiss: () => void
}

/** Inline banner when `resume_session` fails for a sidebar row. */
export const SidebarResumeError = ({
  message,
  onRetry,
  onDismiss,
}: SidebarResumeErrorProps) => {
  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-2 border-t border-stroke-3 bg-danger-subtle px-3 py-2",
      )}
    >
      <p className="flex-1 text-xs text-danger">{message}</p>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-danger"
        onClick={onRetry}
      >
        <RotateCw className="h-3 w-3" aria-hidden />
        Retry
      </Button>
      <IconButton label="Dismiss error" onClick={onDismiss}>
        <X className="h-3.5 w-3.5" aria-hidden />
      </IconButton>
    </div>
  )
}
