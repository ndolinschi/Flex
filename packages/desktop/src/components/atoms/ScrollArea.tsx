import { forwardRef, type ReactNode } from "react"
import { cn } from "../../lib/utils"

type ScrollAreaProps = {
  children: ReactNode
  className?: string
}

export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  ({ children, className }, ref) => {
    return (
      <div
        ref={ref}
        className={cn(
          "min-h-0 overflow-y-auto overscroll-contain",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-border)_transparent]",
          className,
        )}
      >
        {children}
      </div>
    )
  },
)

ScrollArea.displayName = "ScrollArea"
