import { ShieldIcon } from "lucide-react"
import { Toggle } from "@/components/ui/toggle"
import { cn } from "@/lib/utils"
import { Tooltip } from "./Tooltip"

type BypassPermissionsButtonProps = {
  /** Current composer mode — bypass applies in Agent and Debug. */
  composerMode: string
  sessionBypass: boolean
  disabled?: boolean
  onToggle: () => void
}

const allowsBypass = (mode: string): boolean =>
  mode === "agent" || mode === "debug"

/** Session-scoped bypass-permissions shield — pressed Toggle in the composer. */
export const BypassPermissionsButton = ({
  composerMode,
  sessionBypass,
  disabled = false,
  onToggle,
}: BypassPermissionsButtonProps) => {
  const canBypass = allowsBypass(composerMode)
  const pressed = sessionBypass && canBypass
  const shieldLabel = !canBypass
    ? "Bypass applies in Agent or Debug mode"
    : sessionBypass
      ? "Bypass on — agent won't ask (this session + current run)"
      : "Bypass permissions for this session (also covers the current run)"

  return (
    <Tooltip label={shieldLabel}>
      <Toggle
        size="icon-xs"
        pressed={pressed}
        disabled={disabled || !canBypass}
        aria-label={shieldLabel}
        onPressedChange={() => onToggle()}
        className={cn(
          "shrink-0 rounded-full transition-[opacity,background-color,color,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "active:translate-y-px",
          pressed
            ? "bg-orange/15 text-orange opacity-100 hover:bg-orange/25 hover:text-orange active:bg-orange/30 data-pressed:bg-orange/15 data-pressed:text-orange"
            : "text-icon-2 opacity-60 hover:bg-fill-4 hover:opacity-100 active:opacity-100",
        )}
      >
        <ShieldIcon />
      </Toggle>
    </Tooltip>
  )
}
