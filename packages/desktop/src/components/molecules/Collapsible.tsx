import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type CollapsibleProps = {
  open: boolean
  children: ReactNode
  className?: string
}

/**
 * accordion: content stays mounted; expansion animates
 * grid-template-rows (no fixed max-height cap that clips long work groups).
 */
export const Collapsible = ({ open, children, className }: CollapsibleProps) => {
  return (
    <div
      aria-hidden={!open}
      className={cn(
        "grid transition-[grid-template-rows,opacity] duration-[var(--duration-expand)] ease-[var(--easing-in-out)]",
        "[transition-duration:var(--duration-expand),var(--duration-expand-fade)]",
        "motion-reduce:transition-none",
        open
          ? "grid-rows-[1fr] opacity-100"
          : "pointer-events-none grid-rows-[0fr] opacity-0",
        className,
      )}
    >
      <div className="min-h-0 overflow-hidden">{children}</div>
    </div>
  )
}
