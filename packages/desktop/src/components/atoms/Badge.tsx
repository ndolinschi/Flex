import type { ReactNode } from "react"
import { Badge as UiBadge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

type BadgeVariant = "default" | "success" | "warning" | "danger" | "muted"

type BadgeProps = {
  variant?: BadgeVariant
  children: ReactNode
  className?: string
}

/** Map Flex tone names onto shadcn Badge (+ subtle tone classes). */
const variantMap = {
  default: "default",
  success: "secondary",
  warning: "secondary",
  danger: "destructive",
  muted: "secondary",
} as const

const toneClasses: Record<BadgeVariant, string> = {
  default: "bg-accent-subtle text-accent",
  success: "bg-success-subtle text-success",
  warning: "bg-warning-subtle text-warning",
  danger: "",
  muted: "bg-surface-muted text-ink-muted",
}

export const Badge = ({
  variant = "default",
  children,
  className,
}: BadgeProps) => {
  return (
    <UiBadge
      variant={variantMap[variant]}
      className={cn(toneClasses[variant], className)}
    >
      {children}
    </UiBadge>
  )
}
