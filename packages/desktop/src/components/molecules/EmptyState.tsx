import type { ReactNode } from "react"
import { Button } from "../atoms"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { cn } from "@/lib/utils"

type EmptyStateProps = {
  title: string
  description?: string
  icon?: ReactNode
  actionLabel?: string
  onAction?: () => void
  className?: string
}

/** Empty async surface — shadcn Empty composition. */
export const EmptyState = ({
  title,
  description,
  icon,
  actionLabel,
  onAction,
  className,
}: EmptyStateProps) => {
  return (
    <Empty
      className={cn(
        "gap-2 rounded-none border-0 p-4 py-8",
        className,
      )}
    >
      <EmptyHeader className="gap-2">
        {icon ? (
          <EmptyMedia className="mb-0 text-2xl text-ink-faint">{icon}</EmptyMedia>
        ) : null}
        <EmptyTitle className="text-sm font-medium text-ink tracking-normal">
          {title}
        </EmptyTitle>
        {description ? (
          <EmptyDescription className="max-w-sm text-xs text-ink-muted">
            {description}
          </EmptyDescription>
        ) : null}
      </EmptyHeader>
      {actionLabel && onAction ? (
        <EmptyContent>
          <Button size="sm" onClick={onAction}>
            {actionLabel}
          </Button>
        </EmptyContent>
      ) : null}
    </Empty>
  )
}
