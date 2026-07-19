import type { LucideIcon } from "lucide-react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type CommandPaletteRowProps = {
  index: number
  active: boolean
  label: string
  hint?: string
  icon?: LucideIcon
  onActivate: () => void
  onHover: () => void
}

/** Single command/session row inside the CommandPalette list. */
export const CommandPaletteRow = ({
  index,
  active,
  label,
  hint,
  icon: Icon,
  onActivate,
  onHover,
}: CommandPaletteRowProps) => {
  return (
    <Button
      variant="ghost"
      data-index={index}
      onMouseEnter={onHover}
      onClick={onActivate}
      className={cn(
        "h-auto w-full justify-start gap-2 px-3 py-1.5 font-normal",
        "text-sm",
        active ? "bg-fill-4 text-ink" : "text-ink-secondary",
      )}
    >
      {Icon ? (
        <Icon
          className="h-3.5 w-3.5 shrink-0 text-ink-muted"
          aria-hidden
        />
      ) : (
        <span className="h-3.5 w-3.5 shrink-0" />
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {hint ? (
        <span className="shrink-0 truncate text-xs text-ink-faint">
          {hint}
        </span>
      ) : null}
    </Button>
  )
}
