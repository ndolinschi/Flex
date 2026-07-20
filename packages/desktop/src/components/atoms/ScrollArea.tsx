import type { ReactNode } from "react"
import { ScrollArea as ScrollAreaPrimitive } from "@/components/ui/scroll-area"

type ScrollAreaProps = {
  children: ReactNode
  className?: string
}

export const ScrollArea = ({ children, className }: ScrollAreaProps) => (
  <ScrollAreaPrimitive className={className}>{children}</ScrollAreaPrimitive>
)
