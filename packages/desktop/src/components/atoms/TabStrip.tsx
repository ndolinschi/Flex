import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

export const TabStrip = ({ children, className, ...props }: TabStripProps) => {
  return (
    <div
      role="tablist"
      className={cn(
        /* Default = nested tool chrome (30px). Top content TabStrip overrides
           to --titlebar-height + glass-titleband (see ContentPane). */
        "flex h-[var(--header-height)] min-w-0 shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}
