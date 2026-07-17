import type { MouseEvent as ReactMouseEvent, DragEvent as ReactDragEvent } from "react"
import { X } from "@/components/icons"
import { cn } from "../../lib/utils"

type TabCloseProps = {
  label: string
  onClose: () => void
  /** Also expand on `group-focus-within` (file chips / keyboard). */
  revealOnFocusWithin?: boolean
  className?: string
}

/** Hover-collapse close control shared by panel tabs and file chips.
 * Collapses to zero width at rest and expands on group hover. */
export const TabClose = ({
  label,
  onClose,
  revealOnFocusWithin = false,
  className,
}: TabCloseProps) => {
  return (
    <span
      role="button"
      aria-label={label}
      tabIndex={-1}
      onClick={(e: ReactMouseEvent) => {
        e.stopPropagation()
        onClose()
      }}
      onMouseDown={(e: ReactMouseEvent) => {
        // Keep tab drag from starting when grabbing the close control.
        e.stopPropagation()
      }}
      onDragStart={(e: ReactDragEvent) => {
        e.preventDefault()
        e.stopPropagation()
      }}
      className={cn(
        "ml-0 max-w-0 shrink-0 overflow-hidden rounded-sm p-0 opacity-0",
        "transition-[max-width,margin,padding,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
        "hover:bg-fill-1 group-hover:ml-0.5 group-hover:max-w-[1rem] group-hover:p-0.5 group-hover:opacity-100",
        revealOnFocusWithin &&
          "group-focus-within:ml-0.5 group-focus-within:max-w-[1rem] group-focus-within:p-0.5 group-focus-within:opacity-100",
        className,
      )}
    >
      <X className="h-3 w-3" aria-hidden />
    </span>
  )
}
