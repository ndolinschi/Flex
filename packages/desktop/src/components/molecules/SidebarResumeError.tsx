import { AlertCircleIcon, RotateCwIcon, XIcon } from "lucide-react"
import {
  Alert,
  AlertAction,
  AlertDescription,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
      className={cn("rounded-none border-x-0 border-b-0 border-t")}
    >
      <AlertCircleIcon />
      <AlertDescription className="text-xs text-destructive">
        {message}
      </AlertDescription>
      <AlertAction className="flex items-center gap-0.5">
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="text-destructive hover:bg-destructive/15 hover:text-destructive"
          onClick={onRetry}
        >
          <RotateCwIcon data-icon="inline-start" />
          Retry
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Dismiss error"
          className="text-destructive hover:bg-destructive/15 hover:text-destructive"
          onClick={onDismiss}
        >
          <XIcon />
        </Button>
      </AlertAction>
    </Alert>
  )
}
