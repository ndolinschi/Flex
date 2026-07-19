import type { ComponentType } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type SidebarActionRowProps = {
  icon: ComponentType<{
    className?: string
    "aria-hidden"?: boolean
    "data-icon"?: string
  }>
  label: string
  kbd?: string
  /** Outlink marker (e.g. `ArrowUpRight`) for rows that open a separate pane
   * — rendered right-aligned in place of the `kbd` slot. */
  trailingIcon?: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  onClick?: () => void
  disabled?: boolean
}

/** 28px sidebar action row: icon + label flush start + trailing shortcut. */
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
      size="sm"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "h-7 w-full justify-start gap-1.5 rounded-sm px-2 font-normal",
        "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      <Icon data-icon="inline-start" className="text-muted-foreground" aria-hidden />
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      {TrailingIcon ? (
        <TrailingIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden />
      ) : kbd ? (
        <kbd className="shrink-0 font-sans text-xs tracking-[var(--tracking-caption)] text-muted-foreground">
          {kbd}
        </kbd>
      ) : null}
    </Button>
  )
}
