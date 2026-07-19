import { ShieldIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
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

/** Session-scoped bypass-permissions shield — shadcn Button icon. */
export const BypassPermissionsButton = ({
  composerMode,
  sessionBypass,
  disabled = false,
  onToggle,
}: BypassPermissionsButtonProps) => {
  const canBypass = allowsBypass(composerMode)
  const shieldLabel = !canBypass
    ? "Bypass applies in Agent or Debug mode"
    : sessionBypass
      ? "Bypass on — agent won't ask (this session + current run)"
      : "Bypass permissions for this session (also covers the current run)"

  return (
    <Tooltip label={shieldLabel}>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        disabled={disabled || !canBypass}
        aria-label={shieldLabel}
        aria-pressed={sessionBypass && canBypass}
        onClick={onToggle}
        className={cn(
          "size-6 shrink-0 rounded-full",
          sessionBypass && canBypass
            ? "bg-orange/15 text-orange opacity-100 hover:bg-orange/25 hover:text-orange"
            : "text-icon-2 opacity-50 hover:bg-fill-3 hover:opacity-80",
        )}
      >
        <ShieldIcon />
      </Button>
    </Tooltip>
  )
}
