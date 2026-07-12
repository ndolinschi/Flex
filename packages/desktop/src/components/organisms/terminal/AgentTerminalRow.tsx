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
        "group mx-1 flex cursor-pointer items-center gap-1.5 rounded-sm px-2 py-1 text-sm",
        selected ? "bg-fill-2 text-ink" : "hover:bg-fill-4",
      )}
    >
      <InfinityIcon className="h-3.5 w-3.5 shrink-0 text-yellow" aria-hidden />
      <span className="min-w-0 flex-1 truncate">Agent terminal</span>
    </div>
  )
}
