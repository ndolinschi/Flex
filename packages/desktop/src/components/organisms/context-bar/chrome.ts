import { cn } from "../../../lib/utils"

/** Quiet ContextBar chrome pill — folder / branch / isolation triggers.
 * Not a form InputGroup: closed state is a ghost button like Cursor. */
export const contextBarTriggerClass = (...extra: Array<string | false | null | undefined>) =>
  cn(
    "h-6 max-w-[10rem] shrink-0 gap-1 rounded-md border-0 bg-transparent px-1.5",
    "text-sm font-normal text-ink-muted opacity-80 shadow-none outline-none",
    "transition-[color,opacity,background-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
    "hover:bg-fill-4 hover:text-ink hover:opacity-100",
    "focus-visible:bg-fill-4 focus-visible:opacity-100",
    "data-popup-open:bg-fill-4 data-popup-open:opacity-100",
    "data-open:bg-fill-4 data-open:opacity-100",
    "disabled:pointer-events-none disabled:opacity-50",
    "[&_svg]:shrink-0",
    ...extra,
  )
