import type { MouseEvent as ReactMouseEvent, DragEvent as ReactDragEvent } from "react"
import { X } from "lucide-react"
import { cn } from "../../lib/utils"

type TabCloseProps = {
  label: string
  onClose: () => void
  revealOnFocusWithin?: boolean
  className?: string
}

export const TabClose = ({
  label,
  onClose,
  revealOnFocusWithin = false,
  className,
}: TabCloseProps) => {
  return (
    <button
      type="button"
      aria-label={label}
      tabIndex={revealOnFocusWithin ? 0 : -1}
      onClick={(e: ReactMouseEvent) => {
        e.stopPropagation()
        onClose()
      }}
      onMouseDown={(e: ReactMouseEvent) => {
        e.stopPropagation()
      }}
      onDragStart={(e: ReactDragEvent) => {
        e.preventDefault()
        e.stopPropagation()
      }}
      className={cn(
        "pointer-events-none ml-0.5 w-4 shrink-0 overflow-hidden rounded-sm p-0.5 opacity-0",
        "transition-opacity duration-[var(--duration-fast)] ease-[var(--easing-default)] motion-reduce:transition-none",
        "hover:bg-fill-4 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2 group-hover:pointer-events-auto group-hover:opacity-100",
        revealOnFocusWithin &&
          "group-focus-within:pointer-events-auto group-focus-within:opacity-100",
        className,
      )}
      data-tab-no-drag
    >
      <X className="h-3 w-3" aria-hidden />
    </button>
  )
}
