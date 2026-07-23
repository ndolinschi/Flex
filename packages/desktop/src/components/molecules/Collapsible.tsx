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
  /** When true, keep closed content mounted (default false for timeline perf). */
  keepMounted?: boolean
}

export const Collapsible = ({
  open,
  children,
  className,
  keepMounted = false,
}: CollapsibleProps) => {
  return (
    <CollapsibleRoot open={open}>
      <CollapsibleContent
        keepMounted={keepMounted}
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
