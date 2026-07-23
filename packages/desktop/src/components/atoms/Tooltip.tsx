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

export const Tooltip = ({ label, side = "top", children }: TooltipProps) => (
  <TooltipRoot>
    <TooltipTrigger render={children} />
    <TooltipContent side={side}>{label}</TooltipContent>
  </TooltipRoot>
)
