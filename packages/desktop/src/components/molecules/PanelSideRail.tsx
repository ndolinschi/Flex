import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

export type PanelSideRailWidth = 160 | 180

type PanelSideRailProps = {
  children: ReactNode
  /** Terminal uses 160; Database / Components / Artifacts use 180. */
  width?: PanelSideRailWidth
  /** Optional section header row above the list. */
  header?: ReactNode
  className?: string
}

/**
 * Left inventory rail shared by Terminal / Database / Components / Artifacts.
 * Border-r + bg-panel, full height column.
 */
export const PanelSideRail = ({
  children,
  width = 180,
  header,
  className,
}: PanelSideRailProps) => (
  <div
    className={cn(
      "flex shrink-0 flex-col border-r border-stroke-3 bg-panel",
      width === 160 ? "w-[160px]" : "w-[180px]",
      className,
    )}
  >
    {header != null ? (
      <div className="flex h-7 shrink-0 items-center px-2.5 text-xs text-ink-muted">
        {header}
      </div>
    ) : null}
    {children}
  </div>
)
