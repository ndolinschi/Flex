import type { ComponentType } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import { Spinner } from "../atoms"

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

/** Sidebar nav action row (Cursor Agents Web 2026-07-23):
 * `h-8` · `px-2` · `gap-2` · `rounded-sm` (6) · hover `bg-quaternary`.
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
        // Production nav: h-8 gap-2 px-2 rounded-sm — same gutter as agent-row.
        "h-8 w-full justify-start gap-2 rounded-sm px-2 font-medium",
        "text-ink hover:bg-[var(--color-bg-quaternary-opaque)] hover:text-ink",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
      )}
    >
      {loading ? (
        <Spinner
          size="sm"
          label={`${label} in progress`}
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
        <Kbd className="ml-auto shrink-0">{kbd}</Kbd>
      ) : null}
    </Button>
  )
}
