import { useEffect, useRef } from "react"
import { useTerminal } from "../../../hooks/useTerminal"
import { cn } from "../../../lib/utils"

export const TerminalInstance = ({
  id,
  active,
  readOnly,
}: {
  id: string
  active: boolean
  readOnly?: boolean
}) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const { fit } = useTerminal(id, containerRef, active, { readOnly })

  useEffect(() => {
    if (active) fit()
  }, [active, fit])

  return (
    <div
      className={cn("glass-terminal-wrapper h-full w-full", !active && "hidden")}
      onMouseDown={() => {
        const el = containerRef.current?.querySelector("textarea")
        el?.focus()
      }}
    >
      <div ref={containerRef} className="h-full w-full" />
    </div>
  )
}
