import {
  cloneElement,
  isValidElement,
  useEffect,
  useRef,
  useState,
  type ReactElement,
} from "react"
import { createPortal } from "react-dom"
import {
  isProgrammaticScroll,
  isTimelineScrollEvent,
} from "../../lib/programmaticScroll"
import { cn } from "../../lib/utils"

type TooltipSide = "top" | "bottom" | "right"

type TooltipProps = {
  label: string
  side?: TooltipSide
  children: ReactElement
}

const GAP = 6
const MARGIN = 8

/** Delayed hover/focus tooltip, portal-rendered near the child's rect.
 * Dependency-free, single component — no context. */
export const Tooltip = ({ label, side = "top", children }: TooltipProps) => {
  const [rect, setRect] = useState<DOMRect | null>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearTimer = () => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }
  }

  const show = (target: HTMLElement) => {
    clearTimer()
    timerRef.current = setTimeout(() => {
      setRect(target.getBoundingClientRect())
    }, 500)
  }

  const hide = () => {
    clearTimer()
    setRect(null)
  }

  // Hide on user scroll while visible (capture phase catches inner scrollers).
  // Ignore timeline stick-to-bottom + virtualizer followOnAppend scrolls.
  useEffect(() => {
    if (!rect) return
    const onScroll = (e: Event) => {
      if (isProgrammaticScroll() || isTimelineScrollEvent(e)) return
      setRect(null)
    }
    window.addEventListener("scroll", onScroll, true)
    return () => window.removeEventListener("scroll", onScroll, true)
  }, [rect])

  if (!isValidElement(children)) return children

  const childProps = children.props as Record<string, unknown>

  const wrapped = cloneElement(children, {
    onMouseEnter: (e: React.MouseEvent) => {
      ;(childProps.onMouseEnter as ((e: React.MouseEvent) => void) | undefined)?.(e)
      show(e.currentTarget as HTMLElement)
    },
    onMouseLeave: (e: React.MouseEvent) => {
      ;(childProps.onMouseLeave as ((e: React.MouseEvent) => void) | undefined)?.(e)
      hide()
    },
    onMouseDown: (e: React.MouseEvent) => {
      ;(childProps.onMouseDown as ((e: React.MouseEvent) => void) | undefined)?.(e)
      hide()
    },
    onFocus: (e: React.FocusEvent) => {
      ;(childProps.onFocus as ((e: React.FocusEvent) => void) | undefined)?.(e)
      show(e.currentTarget as HTMLElement)
    },
    onBlur: (e: React.FocusEvent) => {
      ;(childProps.onBlur as ((e: React.FocusEvent) => void) | undefined)?.(e)
      hide()
    },
  } as Record<string, unknown>)

  if (!rect) return wrapped

  let top: number
  let left: number
  let transform: string

  if (side === "right") {
    top = rect.top + rect.height / 2
    left = rect.right + GAP
    transform = "translateY(-50%)"
    if (left > window.innerWidth - MARGIN) {
      // Flip to left side of the child if it would go offscreen.
      left = rect.left - GAP
      transform = "translate(-100%, -50%)"
    }
  } else if (side === "bottom") {
    top = rect.bottom + GAP
    left = rect.left + rect.width / 2
    transform = "translateX(-50%)"
    if (top > window.innerHeight - MARGIN) {
      top = rect.top - GAP
      transform = "translate(-50%, -100%)"
    }
  } else {
    // default "top", auto-flip to bottom if offscreen
    top = rect.top - GAP
    left = rect.left + rect.width / 2
    transform = "translate(-50%, -100%)"
    if (top < MARGIN) {
      top = rect.bottom + GAP
      transform = "translateX(-50%)"
    }
  }

  return (
    <>
      {wrapped}
      {createPortal(
        <div
          role="tooltip"
          style={{ position: "fixed", top, left, transform }}
          className={cn(
            "z-[1100] rounded-md border border-stroke-3 bg-panel px-2 py-1",
            "text-xs text-ink-secondary shadow-sm",
            "pointer-events-none whitespace-nowrap",
          )}
        >
          {label}
        </div>,
        document.body,
      )}
    </>
  )
}
