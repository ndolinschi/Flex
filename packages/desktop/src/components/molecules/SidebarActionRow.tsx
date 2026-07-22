import type { ComponentType } from "react"
import { Loader2 } from "lucide-react"
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
  /** Swap the leading icon for a spinner while an async action runs. */
  loading?: boolean
}

/** Sidebar action row density (Cursor agent-sidebar-cell rhythm):
 * `h-7` · `px-2` · `gap-1.5` · `rounded-sm` (6) · hover `fill-4`.
 * Icon + label flush start + trailing shortcut — no flex-1 on the label
 * (that + UA `text-align:center` centers the text mid-row). */
export const SidebarActionRow = ({
  icon: Icon,
  label,
  kbd,
  trailingIcon: TrailingIcon,
  onClick,
  disabled = false,
  loading = false,
}: SidebarActionRowProps) => {
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={onClick}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn(
        // Override Button sm (rounded-md / px-2.5) to sidebar cell recipe.
        "h-7 w-full justify-start gap-1.5 rounded-sm px-2 font-normal",
        "text-ink-secondary hover:bg-fill-4 hover:text-ink",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
      )}
    >
      {loading ? (
        <Loader2
          data-icon="inline-start"
          className="animate-spin text-ink-secondary"
          aria-hidden
        />
      ) : (
        <Icon data-icon="inline-start" className="text-ink-secondary" aria-hidden />
      )}
      <span className="min-w-0 truncate">{label}</span>
      {TrailingIcon ? (
        <TrailingIcon
          className="ml-auto size-3 shrink-0 text-ink-secondary"
          aria-hidden
        />
      ) : kbd ? (
        <kbd className="ml-auto shrink-0 font-sans text-xs tracking-[var(--tracking-caption)] text-ink-secondary">
          {kbd}
        </kbd>
      ) : null}
    </Button>
  )
}
