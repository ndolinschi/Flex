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

/** Single command/session row inside the CommandPalette / Open-tab list. */
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
        // Real menu row: icon + label flush start (not justify-center).
        "h-8 w-full justify-start gap-1.5 rounded-md px-2.5 py-0 font-normal",
        "text-sm",
        active
          ? "bg-muted text-foreground hover:bg-muted"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      {Icon ? (
        <Icon data-icon="inline-start" className="text-muted-foreground" aria-hidden />
      ) : (
        <span className="size-3.5 shrink-0" aria-hidden />
      )}
      <span className="min-w-0 truncate">{label}</span>
      {hint ? (
        <span className="ml-auto shrink-0 truncate text-xs text-muted-foreground">
          {hint}
        </span>
      ) : null}
    </Button>
  )
}
