import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type BadgeVariant = "default" | "success" | "warning" | "danger" | "muted"

type BadgeProps = {
  variant?: BadgeVariant
  children: ReactNode
  className?: string
}

const variantClasses: Record<BadgeVariant, string> = {
  default: "bg-accent-subtle text-accent",
  success: "bg-success-subtle text-success",
  warning: "bg-warning-subtle text-warning",
  danger: "bg-danger-subtle text-danger",
  muted: "bg-surface-muted text-ink-muted",
}

export const Badge = ({
  variant = "default",
  children,
  className,
}: BadgeProps) => {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-1.5 py-px text-xs font-medium",
        variantClasses[variant],
        className,
      )}
    >
      {children}
    </span>
  )
}
