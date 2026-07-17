import {
  isValidElement,
  useEffect,
  useState,
  type ReactElement,
} from "react"
import {
  isProgrammaticScroll,
  isTimelineScrollEvent,
} from "../../lib/programmaticScroll"
import {
  Tooltip as TooltipRoot,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"

type TooltipSide = "top" | "bottom" | "right"

type TooltipProps = {
  label: string
  side?: TooltipSide
  children: ReactElement
}

/** Delayed hover/focus tooltip over shadcn/Radix.
 * Local Provider keeps unit tests / isolated mounts working; App also mounts
 * a root Provider for shared delay. Dismisses on user scroll; ignores timeline
 * stick-to-bottom / virtualizer scrolls. */
export const Tooltip = ({ label, side = "top", children }: TooltipProps) => {
  const [open, setOpen] = useState(false)

  useEffect(() => {
    if (!open) return
    const onScroll = (e: Event) => {
      if (isProgrammaticScroll() || isTimelineScrollEvent(e)) return
      setOpen(false)
    }
    window.addEventListener("scroll", onScroll, true)
    return () => window.removeEventListener("scroll", onScroll, true)
  }, [open])

  if (!isValidElement(children)) return children

  return (
    <TooltipProvider delayDuration={500}>
      <TooltipRoot open={open} onOpenChange={setOpen}>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent side={side}>{label}</TooltipContent>
      </TooltipRoot>
    </TooltipProvider>
  )
}
