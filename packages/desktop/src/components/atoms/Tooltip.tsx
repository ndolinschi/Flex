import type { ReactElement } from "react"
import {
  Tooltip as TooltipRoot,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip"

type TooltipSide = "top" | "bottom" | "right"

type TooltipProps = {
  label: string
  side?: TooltipSide
  children: ReactElement
}

/** Preserves the original `{ label, side?, children }` public API while
 * delegating to shadcn's Base UI Tooltip. Delay is provided by the
 * `TooltipProvider` mounted in main.tsx (500 ms). */
export const Tooltip = ({ label, side = "top", children }: TooltipProps) => (
  <TooltipRoot>
    <TooltipTrigger render={children} />
    <TooltipContent side={side}>{label}</TooltipContent>
  </TooltipRoot>
)
