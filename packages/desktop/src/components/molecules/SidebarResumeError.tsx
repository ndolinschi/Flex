import { AlertCircleIcon, RotateCwIcon, XIcon } from "lucide-react"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

type SidebarResumeErrorProps = {
  message: string
  onRetry: () => void
  onDismiss: () => void
}

export const SidebarResumeError = ({
  message,
  onRetry,
  onDismiss,
}: SidebarResumeErrorProps) => {
  return (
    <Alert
      variant="destructive"
      className={cn(
        "rounded-none border-x-0 border-b-0 border-t border-danger/15 bg-danger-subtle/70 py-1.5 text-danger",
      )}
    >
      <AlertCircleIcon className="size-3.5 opacity-80" aria-hidden />
      <AlertTitle className="sr-only">Resume error</AlertTitle>
      <AlertDescription className="text-xs leading-snug text-danger/90">
        {message}
      </AlertDescription>
      <AlertAction className="flex items-center gap-0.5">
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="text-danger/80 opacity-70 hover:bg-danger/10 hover:text-danger hover:opacity-100"
          onClick={onRetry}
        >
          <RotateCwIcon data-icon="inline-start" className="size-3" aria-hidden />
          Retry
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Dismiss error"
          className="text-danger/80 opacity-70 hover:bg-danger/10 hover:text-danger hover:opacity-100"
          onClick={onDismiss}
        >
          <XIcon className="size-3.5" aria-hidden />
        </Button>
      </AlertAction>
    </Alert>
  )
}
