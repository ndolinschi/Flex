import { TriangleAlert, X } from "@/components/icons"
import { IconButton } from "../atoms"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { cn } from "@/lib/utils"

type ErrorBannerProps = {
  message: string
  onDismiss?: () => void
  className?: string
}

/** Inline error callout — shadcn Alert (destructive). */
export const ErrorBanner = ({ message, onDismiss, className }: ErrorBannerProps) => {
  if (!message) return null

  return (
    <Alert
      variant="destructive"
      className={cn(
        "border-danger/20 bg-danger-subtle text-danger",
        className,
      )}
    >
      <TriangleAlert aria-hidden />
      <AlertTitle className="sr-only">Error</AlertTitle>
      <AlertDescription className="text-sm text-danger">{message}</AlertDescription>
      {onDismiss ? (
        <AlertAction>
          <IconButton label="Dismiss error" onClick={onDismiss}>
            <X className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </AlertAction>
      ) : null}
    </Alert>
  )
}
