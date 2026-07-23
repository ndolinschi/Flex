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
        // Production terminal session pill: h-6 rounded-md, selected quaternary.
        "group mx-1.5 flex h-6 w-[calc(100%-12px)] cursor-pointer items-center gap-1.5 rounded-md px-2 text-sm",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        selected
          ? "bg-[var(--color-bg-quaternary-opaque)] text-ink hover:bg-[var(--color-bg-quaternary-opaque)]"
          : "text-ink-secondary hover:bg-bg-quaternary hover:text-ink",
      )}
    >
      <TerminalIcon
        className="size-3.5 shrink-0 text-icon-3"
        strokeWidth={1.5}
        aria-hidden
      />
      <span className="min-w-0 flex-1 truncate font-medium">{title}</span>
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
