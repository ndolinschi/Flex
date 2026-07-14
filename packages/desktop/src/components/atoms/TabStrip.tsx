import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

/** Horizontal open-tabs strip — right-panel header chrome. */
export const TabStrip = ({ children, className, ...props }: TabStripProps) => {
  return (
    <div
      role="tablist"
      className={cn(
        "flex h-[var(--header-height)] shrink-0 items-center gap-1 border-b border-stroke-3 px-2",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}
