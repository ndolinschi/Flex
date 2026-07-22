import type { ReactNode } from "react"
import { Badge as BadgePrimitive } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

type BadgeTone = "default" | "success" | "warning" | "danger" | "muted"

type BadgeProps = {
  variant?: BadgeTone
  children: ReactNode
  className?: string
}

const toneClasses: Record<BadgeTone, string> = {
  default: "bg-primary/15 text-primary border-transparent",
  success: "bg-success-subtle text-success border-transparent",
  warning: "bg-warning-subtle text-warning border-transparent",
  danger: "bg-destructive/10 text-destructive border-transparent",
  muted: "bg-fill-3 text-ink-muted border-transparent",
}

export const Badge = ({ variant = "default", children, className }: BadgeProps) => (
  <BadgePrimitive
    variant="outline"
    className={cn(
      "tracking-[var(--tracking-caption)]",
      toneClasses[variant],
      className,
    )}
  >
    {children}
  </BadgePrimitive>
)
