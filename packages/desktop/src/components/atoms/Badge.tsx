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
  default: "border-transparent bg-bg-quaternary text-ink-secondary",
  success: "border-transparent bg-bg-success-quaternary text-text-success",
  warning: "border-transparent bg-bg-warn-quaternary text-text-warn",
  danger: "border-transparent bg-bg-danger-quaternary text-text-danger",
  muted: "border-transparent bg-fill-3 text-ink-muted",
}

export const Badge = ({ variant = "default", children, className }: BadgeProps) => (
  <BadgePrimitive
    variant="outline"
    className={cn(
      "h-5 rounded-full px-2 text-xs font-medium tracking-[var(--tracking-caption)]",
      toneClasses[variant],
      className,
    )}
  >
    {children}
  </BadgePrimitive>
)
