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
  trailingIcon?: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  onClick?: () => void
  disabled?: boolean
  loading?: boolean
}

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
        "h-8 w-full justify-start gap-2 rounded-sm px-2.5 font-medium",
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
