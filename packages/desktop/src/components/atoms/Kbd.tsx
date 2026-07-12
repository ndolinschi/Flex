import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type KbdProps = {
  children: ReactNode
  className?: string
}

export const Kbd = ({ children, className }: KbdProps) => {
  return (
    <kbd
      className={cn(
        "inline-flex items-center rounded border border-border bg-surface-muted",
        "px-1.5 py-0.5 font-mono text-xs text-ink-muted",
        className,
      )}
    >
      {children}
    </kbd>
  )
}
