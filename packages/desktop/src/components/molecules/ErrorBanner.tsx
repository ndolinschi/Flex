import { AlertCircleIcon, XIcon } from "lucide-react"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

type ErrorBannerProps = {
  message: string
  onDismiss?: () => void
  className?: string
  /** Optional short title above the message. */
  title?: string
}

/**
 * App-wide inline error callout — shadcn Alert (destructive).
 * Prefer this over ad-hoc danger strips.
 */
export const ErrorBanner = ({
  message,
  onDismiss,
  className,
  title,
}: ErrorBannerProps) => {
  if (!message) return null

  return (
    <Alert variant="destructive" className={cn(className)}>
      <AlertCircleIcon />
      {title ? <AlertTitle>{title}</AlertTitle> : null}
      <AlertDescription className="text-destructive">{message}</AlertDescription>
      {onDismiss ? (
        <AlertAction>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Dismiss error"
            onClick={onDismiss}
            className="text-destructive hover:bg-destructive/15 hover:text-destructive"
          >
            <XIcon />
          </Button>
        </AlertAction>
      ) : null}
    </Alert>
  )
}
