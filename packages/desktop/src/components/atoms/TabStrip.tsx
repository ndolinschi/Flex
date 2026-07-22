import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

/** Horizontal open-tabs strip — content pane chrome (and any embedded
 * open-buffer strips). Height = `--header-height` (30px); default `px-2.5` +
 * `gap-1.5` + bottom hairline. Callers may override padding/border/flex for
 * embedding, but keep gap/height unless a surface documents an exception.
 * Use a `min-w-0 flex-1 overflow-x-auto` child for the tab pills so trailing
 * actions (+ / close pane) stay pinned. */
export const TabStrip = ({ children, className, ...props }: TabStripProps) => {
  return (
    <div
      role="tablist"
      className={cn(
        "flex h-[var(--header-height)] min-w-0 shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}
