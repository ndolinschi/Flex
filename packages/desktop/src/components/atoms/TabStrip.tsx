import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

/** Horizontal open-tabs strip — right-panel header chrome.
 * `px-2.5` + `gap-1.5` keep pill tabs inset from the panel edge and from
 * each other; pair with `Tab` md at `h-6` so selected fills clear the
 * strip's top edge and bottom border (Feel: Quiet chrome / 4px grid). */
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
