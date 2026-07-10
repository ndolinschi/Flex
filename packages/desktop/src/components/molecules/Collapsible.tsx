import type { ReactNode } from "react"
import { useEffect, useRef } from "react"
import { cn } from "../../lib/utils"

type CollapsibleProps = {
  open: boolean
  children: ReactNode
  className?: string
}

/**
 * Cursor-style accordion: content stays mounted; expansion animates
 * grid-template-rows (no fixed max-height cap that clips long work groups).
 */
export const Collapsible = ({ open, children, className }: CollapsibleProps) => {
  const contentRef = useRef<HTMLDivElement>(null)

  // #region agent log
  useEffect(() => {
    if (!open || !contentRef.current) return
    const h = contentRef.current.scrollHeight
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H6",
        location: "Collapsible.tsx",
        message: "collapsible height (grid, no 500px cap)",
        data: {
          scrollHeight: h,
          clippedByCap: false,
          strategy: "grid-rows",
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
  }, [open, children])
  // #endregion

  return (
    <div
      aria-hidden={!open}
      className={cn(
        "grid transition-[grid-template-rows,opacity] duration-[var(--duration-expand)] ease-[var(--easing-in-out)]",
        "[transition-duration:var(--duration-expand),var(--duration-expand-fade)]",
        "motion-reduce:transition-none",
        open
          ? "grid-rows-[1fr] opacity-100"
          : "pointer-events-none grid-rows-[0fr] opacity-0",
        className,
      )}
    >
      <div ref={contentRef} className="min-h-0 overflow-hidden">
        {children}
      </div>
    </div>
  )
}
