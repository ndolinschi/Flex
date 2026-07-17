import type { Icon } from "@/components/icons"
import { cn } from "../../lib/utils"

type CommandPaletteRowProps = {
  index: number
  active: boolean
  label: string
  hint?: string
  icon?: Icon
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
    <button
      type="button"
      data-index={index}
      onMouseEnter={onHover}
      onClick={onActivate}
      className={cn(
        "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm",
        "transition-colors duration-[var(--duration-fast)]",
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
    </button>
  )
}
