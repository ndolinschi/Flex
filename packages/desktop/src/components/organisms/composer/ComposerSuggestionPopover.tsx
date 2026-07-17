import { useLayoutEffect, useState, type ReactNode, type RefObject } from "react"
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"

type ComposerSuggestionPopoverProps = {
  open: boolean
  onClose: () => void
  anchorRef: RefObject<HTMLElement | null>
  children: ReactNode
  "aria-label": string
  className?: string
}

/** Portaled suggestion tray anchored to the composer bubble.
 * Never steals focus (`onOpenAutoFocus` / `onCloseAutoFocus` prevented) so
 * the textarea keeps typing + ↑↓/Enter filtering. */
export const ComposerSuggestionPopover = ({
  open,
  onClose,
  anchorRef,
  children,
  "aria-label": ariaLabel,
  className,
}: ComposerSuggestionPopoverProps) => {
  const [rect, setRect] = useState<DOMRect | null>(null)

  useLayoutEffect(() => {
    if (!open) {
      setRect(null)
      return
    }
    const el = anchorRef.current
    if (!el) return
    const update = () => setRect(el.getBoundingClientRect())
    update()
    window.addEventListener("resize", update)
    return () => window.removeEventListener("resize", update)
  }, [open, anchorRef])

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      {rect ? (
        <PopoverAnchor asChild>
          <span
            aria-hidden
            className="pointer-events-none fixed size-0"
            style={{
              left: rect.left,
              top: rect.top,
              width: rect.width,
              height: 0,
            }}
          />
        </PopoverAnchor>
      ) : null}
      <PopoverContent
        side="top"
        align="start"
        sideOffset={6}
        onOpenAutoFocus={(e) => e.preventDefault()}
        onCloseAutoFocus={(e) => e.preventDefault()}
        role="listbox"
        aria-label={ariaLabel}
        style={rect ? { width: rect.width } : undefined}
        className={cn(
          "max-h-none gap-0 overflow-hidden rounded-md border-0 p-0 shadow-[var(--shadow-popover)]",
          className,
        )}
      >
        {children}
      </PopoverContent>
    </Popover>
  )
}
