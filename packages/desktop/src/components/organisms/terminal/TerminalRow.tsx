import { Terminal as TerminalIcon, X } from "lucide-react"
import { cn } from "../../../lib/utils"
import { formatElapsed } from "./time"
import { Button } from "@/components/ui/button"

export const TerminalRow = ({
  title,
  createdAtMs,
  selected,
  now,
  onSelect,
  onRequestClose,
}: {
  title: string
  createdAtMs: number
  selected: boolean
  now: number
  onSelect: () => void
  onRequestClose: () => void
}) => {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault()
          onSelect()
        }
      }}
      className={cn(
        "group flex w-full cursor-pointer items-center gap-1.5 px-2.5 py-1.5 text-xs",
        selected ? "bg-fill-2 text-ink hover:bg-fill-2" : "hover:bg-fill-4",
      )}
    >
      <TerminalIcon className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{title}</span>
      <span className="shrink-0 text-xs text-ink-muted group-hover:hidden">
        {formatElapsed(createdAtMs, now)}
      </span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Close terminal"
        title="Close Terminal"
        onClick={(e) => {
          e.stopPropagation()
          onRequestClose()
        }}
        className={cn(
          "hidden h-4 w-4 shrink-0 rounded-sm text-ink-muted",
          "hover:bg-fill-4 hover:text-ink group-hover:flex group-focus-within:flex",
        )}
      >
        <X className="h-3 w-3" aria-hidden />
      </Button>
    </div>
  )
}
