import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

/** Horizontal open-tabs strip — shared by right-panel header and AppHeader
 * chat tabs. Default `px-2.5` + `gap-1.5` + bottom border for panel chrome;
 * callers override (`border-b-0`, `flex-1`, overflow) when embedding. */
export const TabStrip = ({ children, className, ...props }: TabStripProps) => {
  return (
    <div
      role="tablist"
      className={cn(
        "flex h-[var(--header-height)] shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}
