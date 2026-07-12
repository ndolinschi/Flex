import type { ReactNode } from "react"
import { cn } from "../../lib/utils"
import { Button } from "../atoms"

type EmptyStateProps = {
  title: string
  description?: string
  icon?: ReactNode
  actionLabel?: string
  onAction?: () => void
  className?: string
}

export const EmptyState = ({
  title,
  description,
  icon,
  actionLabel,
  onAction,
  className,
}: EmptyStateProps) => {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-2 px-4 py-8 text-center",
        className,
      )}
    >
      {icon ? (
        <div className="text-2xl text-ink-faint" aria-hidden="true">
          {icon}
        </div>
      ) : null}
      <h3 className="text-sm font-medium text-ink">{title}</h3>
      {description ? (
        <p className="max-w-sm text-xs text-ink-muted">{description}</p>
      ) : null}
      {actionLabel && onAction ? (
        <Button size="sm" onClick={onAction} className="mt-1">
          {actionLabel}
        </Button>
      ) : null}
    </div>
  )
}
