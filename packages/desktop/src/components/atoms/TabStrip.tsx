import type { HTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type TabStripProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
}

/** Horizontal open-tabs strip — content pane top chrome (and any embedded
 * open-buffer strips). Default height = `--header-height` (30px); the main
 * content-pane strip overrides to `--titlebar-height` (35px) so tabs sit in
 * the topmost window header flush with the sidebar. Default `px-2.5` +
 * `gap-1.5` + bottom hairline (`stroke-3`). Callers may override
 * padding/border/flex/height for embedding. Tab pills use whisper fills
 * (fill-2 / fill-4) and stay vertically centered with the sidebar mark.
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
