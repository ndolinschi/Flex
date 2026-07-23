import { Infinity as InfinityIcon } from "lucide-react"
import { cn } from "../../../lib/utils"

export const AgentTerminalRow = ({
  selected,
  onSelect,
}: {
  selected: boolean
  onSelect: () => void
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
        // Match TerminalRow production pill (h-6 rounded-md quaternary selected).
        "group mx-1.5 flex h-6 w-[calc(100%-12px)] cursor-pointer items-center gap-1.5 rounded-md px-2 text-sm",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        selected
          ? "bg-[var(--color-bg-quaternary-opaque)] text-ink hover:bg-[var(--color-bg-quaternary-opaque)]"
          : "text-ink-secondary hover:bg-bg-quaternary hover:text-ink",
      )}
    >
      <InfinityIcon
        className="size-3.5 shrink-0 text-yellow"
        strokeWidth={1.5}
        aria-hidden
      />
      <span className="min-w-0 flex-1 truncate font-medium">Agent terminal</span>
    </div>
  )
}
