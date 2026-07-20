import type { ReactNode } from "react"
import {
  Collapsible as CollapsibleRoot,
  CollapsibleContent,
} from "@/components/ui/collapsible"
import { cn } from "../../lib/utils"

type CollapsibleProps = {
  open: boolean
  children: ReactNode
  className?: string
}

/**
 * Controlled animation panel: content stays mounted; expansion animates via
 * grid-template-rows so there's no fixed max-height cap that clips long groups.
 * Thin adapter over ui/collapsible — keeps the same `open`/`children` API
 * used by WorkGroup, ToolStepGroup, CompactionCard, SubagentGroup, etc.
 */
export const Collapsible = ({ open, children, className }: CollapsibleProps) => {
  return (
    <CollapsibleRoot open={open}>
      <CollapsibleContent
        keepMounted
        className={cn(
          "grid transition-[grid-template-rows,opacity]",
          "[transition-duration:var(--duration-expand),var(--duration-expand-fade)]",
          "ease-[var(--easing-in-out)] motion-reduce:transition-none",
          "data-[open]:grid-rows-[1fr] data-[open]:opacity-100",
          "data-[closed]:pointer-events-none data-[closed]:grid-rows-[0fr] data-[closed]:opacity-0",
          className,
        )}
      >
        <div className="min-h-0 overflow-hidden">{children}</div>
      </CollapsibleContent>
    </CollapsibleRoot>
  )
}
