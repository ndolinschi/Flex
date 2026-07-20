import { HighlightedLabel } from "../atoms"
import { cn, formatRelativeTime } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type FuzzySessionRowProps = {
  index: number
  active: boolean
  label: string
  query: string
  updatedAtMs: number
  onActivate: () => void
  onHover: () => void
}

/** Single session row inside the SearchModal list, with fuzzy highlight. */
export const FuzzySessionRow = ({
  index,
  active,
  label,
  query,
  updatedAtMs,
  onActivate,
  onHover,
}: FuzzySessionRowProps) => {
  return (
    <Button
      variant="ghost"
      data-index={index}
      onMouseEnter={onHover}
      onClick={onActivate}
      className={cn(
        "h-auto w-full justify-start gap-2 px-3 py-1.5 font-normal",
        "text-sm",
        active ? "bg-fill-2 text-ink" : "text-ink-secondary hover:bg-fill-4",
      )}
    >
      <span className="min-w-0 flex-1 truncate">
        <HighlightedLabel label={label} query={query} />
      </span>
      <span className="shrink-0 truncate text-xs text-ink-faint">
        {formatRelativeTime(updatedAtMs)}
      </span>
    </Button>
  )
}
