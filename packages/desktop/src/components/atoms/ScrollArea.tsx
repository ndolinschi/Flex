import { forwardRef, type ReactNode } from "react"
import { ScrollArea as UiScrollArea } from "@/components/ui/scroll-area"
import { cn } from "@/lib/utils"

type ScrollAreaProps = {
  children: ReactNode
  className?: string
}

/** Thin wrap over shadcn ScrollArea (Radix viewport + scrollbar). */
export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  ({ children, className }, ref) => {
    return (
      <UiScrollArea
        ref={ref}
        className={cn(
          "min-h-0 overscroll-contain",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-border)_transparent]",
          className,
        )}
      >
        {children}
      </UiScrollArea>
    )
  },
)

ScrollArea.displayName = "ScrollArea"
