import { X } from "lucide-react"
import { cn } from "../../lib/utils"
import { IconButton } from "../atoms"

type ErrorBannerProps = {
  message: string
  onDismiss?: () => void
  className?: string
}

export const ErrorBanner = ({ message, onDismiss, className }: ErrorBannerProps) => {
  if (!message) return null

  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-2 rounded-md border border-danger/20 bg-danger-subtle px-3 py-2",
        className,
      )}
    >
      <p className="flex-1 text-sm text-danger">{message}</p>
      {onDismiss ? (
        <IconButton label="Dismiss error" onClick={onDismiss}>
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      ) : null}
    </div>
  )
}
