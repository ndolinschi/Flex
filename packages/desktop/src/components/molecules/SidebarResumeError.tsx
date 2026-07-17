import { RotateCw, TriangleAlert, X } from "@/components/icons"
import { Button, IconButton } from "../atoms"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { cn } from "@/lib/utils"

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
    <Alert
      variant="destructive"
      className={cn(
        "rounded-none border-0 border-t border-stroke-3 bg-danger-subtle px-2 py-2 text-danger",
        "has-data-[slot=alert-action]:pr-2",
      )}
    >
      <TriangleAlert aria-hidden />
      <AlertTitle className="sr-only">Resume failed</AlertTitle>
      <AlertDescription className="flex flex-wrap items-center gap-2 text-xs text-danger">
        <span className="min-w-0 flex-1">{message}</span>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-1.5 text-danger"
          onClick={onRetry}
        >
          <RotateCw className="h-3 w-3" aria-hidden />
          Retry
        </Button>
      </AlertDescription>
      <AlertAction className="static top-auto right-auto">
        <IconButton label="Dismiss error" onClick={onDismiss}>
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </AlertAction>
    </Alert>
  )
}
