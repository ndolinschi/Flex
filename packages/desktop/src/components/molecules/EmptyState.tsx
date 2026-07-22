import type { ReactNode } from "react"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Button } from "@/components/ui/button"

type EmptyStateProps = {
  title: string
  description?: string
  icon?: ReactNode
  /** Custom action node (wins over `actionLabel` / `onAction`). */
  action?: ReactNode
  actionLabel?: string
  onAction?: () => void
  actionDisabled?: boolean
  className?: string
}

export const EmptyState = ({
  title,
  description,
  icon,
  action,
  actionLabel,
  onAction,
  actionDisabled = false,
  className,
}: EmptyStateProps) => {
  const defaultAction =
    actionLabel && onAction ? (
      <Button variant="secondary" size="sm" onClick={onAction} disabled={actionDisabled}>
        {actionLabel}
      </Button>
    ) : null

  return (
    <Empty className={className}>
      <EmptyHeader className="gap-1.5">
        {icon ? (
          <EmptyMedia variant="icon" className="mb-0" aria-hidden="true">
            {icon}
          </EmptyMedia>
        ) : null}
        <EmptyTitle>{title}</EmptyTitle>
        {description ? (
          <EmptyDescription className="max-w-sm">{description}</EmptyDescription>
        ) : null}
      </EmptyHeader>
      {action || defaultAction ? (
        <EmptyContent className="mt-0.5">{action ?? defaultAction}</EmptyContent>
      ) : null}
    </Empty>
  )
}
