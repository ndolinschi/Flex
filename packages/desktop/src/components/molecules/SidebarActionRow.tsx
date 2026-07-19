import type { ComponentType } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type SidebarActionRowProps = {
  icon: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  label: string
  kbd?: string
  /** Outlink marker (e.g. `ArrowUpRight`) for rows that open a separate pane
   * — rendered right-aligned in place of the `kbd` slot. */
  trailingIcon?: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  onClick?: () => void
  disabled?: boolean
}

/** 28px sidebar action row: icon + label + trailing shortcut. */
export const SidebarActionRow = ({
  icon: Icon,
  label,
  kbd,
  trailingIcon: TrailingIcon,
  onClick,
  disabled = false,
}: SidebarActionRowProps) => {
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "group h-auto min-h-7 w-full justify-start gap-3 rounded-sm px-2 py-1.5",
        "text-sm text-ink-secondary font-normal",
        "hover:bg-fill-4 hover:text-ink",
      )}
    >
      <span className="flex min-w-0 flex-1 items-center gap-1.5">
        <Icon className="h-3.5 w-3.5 shrink-0 text-icon-2" aria-hidden />
        <span className="min-w-0 flex-1 truncate">{label}</span>
      </span>
      {TrailingIcon ? (
        <TrailingIcon className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
      ) : kbd ? (
        <kbd className="shrink-0 font-sans text-xs tracking-[var(--tracking-caption)] text-ink-faint">
          {kbd}
        </kbd>
      ) : null}
    </Button>
  )
}
