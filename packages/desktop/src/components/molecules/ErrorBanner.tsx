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
  title?: string
}

export const ErrorBanner = ({
  message,
  onDismiss,
  className,
  title,
}: ErrorBannerProps) => {
  if (!message) return null

  return (
    <Alert
      variant="destructive"
      className={cn(
        "border-danger/15 bg-danger-subtle/70 py-1.5 text-danger",
        className,
      )}
    >
      <AlertCircleIcon className="size-3.5 opacity-80" aria-hidden />
      {title ? <AlertTitle>{title}</AlertTitle> : (
        <AlertTitle className="sr-only">Error</AlertTitle>
      )}
      <AlertDescription className="text-xs leading-snug text-danger/90">
        {message}
      </AlertDescription>
      {onDismiss ? (
        <AlertAction>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Dismiss error"
            onClick={onDismiss}
            className="text-danger/80 opacity-70 hover:bg-danger/10 hover:text-danger hover:opacity-100"
          >
            <XIcon className="size-3.5" aria-hidden />
          </Button>
        </AlertAction>
      ) : null}
    </Alert>
  )
}
