import { AlertCircleIcon, RefreshCw } from "lucide-react"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "../../lib/utils"
import { toInvokeError } from "../../lib/tauri"
import { EmptyState } from "./EmptyState"

export type ToolQueryErrorProps = {
  /** Raw query error or pre-stringified message. */
  error: unknown
  onRetry?: () => void
  retrying?: boolean
  /**
   * `banner` — strip under tool chrome (partial page still usable).
   * `fill` — full-pane empty-style failure (primary load failed).
   */
  variant?: "banner" | "fill"
  title?: string
  className?: string
  fallbackMessage?: string
}

/** Normalize React Query / invoke errors for tool-tab surfaces. */
export const toolQueryErrorMessage = (
  error: unknown,
  fallback = "Request failed",
): string => {
  const msg = toInvokeError(error).trim()
  return msg || fallback
}

/**
 * Distinguishes query failure from true empty state.
 * Use when a primary list/status load fails so tabs never say
 * “clean” / “none yet” on IPC error.
 */
export const ToolQueryError = ({
  error,
  onRetry,
  retrying = false,
  variant = "fill",
  title = "Couldn't load",
  className,
  fallbackMessage = "Request failed",
}: ToolQueryErrorProps) => {
  const message = toolQueryErrorMessage(error, fallbackMessage)

  if (variant === "banner") {
    return (
      <Alert
        variant="destructive"
        className={cn(
          "shrink-0 rounded-none border-x-0 border-t-0 border-danger/15 bg-danger-subtle/70 px-2.5 py-1.5 text-danger",
          className,
        )}
      >
        <AlertCircleIcon className="size-3.5 opacity-80" aria-hidden />
        <AlertTitle className="text-sm">{title}</AlertTitle>
        <AlertDescription className="text-xs leading-snug text-danger/90">
          {message}
        </AlertDescription>
        {onRetry ? (
          <AlertAction>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-6 gap-1 px-2 text-xs text-danger/90 hover:bg-danger/10 hover:text-danger"
              onClick={onRetry}
              disabled={retrying}
              aria-label="Retry"
            >
              <RefreshCw
                className={cn("size-3", retrying && "animate-spin")}
                aria-hidden
              />
              Retry
            </Button>
          </AlertAction>
        ) : null}
      </Alert>
    )
  }

  return (
    <EmptyState
      className={cn("min-h-0 flex-1 px-2.5", className)}
      icon={<AlertCircleIcon className="h-6 w-6" aria-hidden />}
      title={title}
      description={message}
      action={
        onRetry ? (
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="gap-1.5"
            onClick={onRetry}
            disabled={retrying}
          >
            <RefreshCw
              className={cn("size-3.5", retrying && "animate-spin")}
              aria-hidden
            />
            Retry
          </Button>
        ) : undefined
      }
    />
  )
}
