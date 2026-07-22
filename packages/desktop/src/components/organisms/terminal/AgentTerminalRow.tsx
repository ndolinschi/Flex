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
        "group flex w-full cursor-pointer items-center gap-1.5 px-2.5 py-1.5 text-xs",
        selected ? "bg-fill-2 text-ink hover:bg-fill-2" : "hover:bg-fill-4",
      )}
    >
      <InfinityIcon className="h-3.5 w-3.5 shrink-0 text-yellow" aria-hidden />
      <span className="min-w-0 flex-1 truncate">Agent terminal</span>
    </div>
  )
}
