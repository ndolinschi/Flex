import { Shield } from "lucide-react"
import { cn } from "../../lib/utils"
import { Tooltip } from "./Tooltip"

type BypassPermissionsButtonProps = {
  /** Current composer mode — bypass only applies in agent mode. */
  composerMode: string
  sessionBypass: boolean
  disabled?: boolean
  onToggle: () => void
}

/** Session-scoped bypass-permissions shield control for the composer toolbar. */
export const BypassPermissionsButton = ({
  composerMode,
  sessionBypass,
  disabled = false,
  onToggle,
}: BypassPermissionsButtonProps) => {
  const shieldLabel =
    composerMode !== "agent"
      ? "Bypass applies in Agent mode"
      : sessionBypass
        ? "Bypass on — agent won't ask (this session + current run)"
        : "Bypass permissions for this session (also covers the current run)"

  return (
    <Tooltip label={shieldLabel}>
      <button
        type="button"
        disabled={disabled || composerMode !== "agent"}
        aria-label={shieldLabel}
        aria-pressed={sessionBypass}
        onClick={onToggle}
        className={cn(
          "inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
          "transition-[opacity,background-color,color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "disabled:opacity-30 disabled:pointer-events-none",
          sessionBypass && composerMode === "agent"
            ? "bg-orange/15 text-orange opacity-100 hover:bg-orange/25"
            : "text-icon-2 opacity-50 hover:bg-fill-3 hover:opacity-80",
        )}
      >
        <Shield className="h-3.5 w-3.5" aria-hidden />
      </button>
    </Tooltip>
  )
}
