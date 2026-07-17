import type { ReactNode } from "react"
import { Kbd as UiKbd } from "@/components/ui/kbd"
import { cn } from "@/lib/utils"

type KbdProps = {
  children: ReactNode
  className?: string
}

export const Kbd = ({ children, className }: KbdProps) => {
  return (
    <UiKbd
      className={cn(
        "rounded border border-border bg-surface-muted font-mono text-ink-muted",
        className,
      )}
    >
      {children}
    </UiKbd>
  )
}
