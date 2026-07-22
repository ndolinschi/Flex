import { useEffect, useRef, useState } from "react"
import { Shuffle, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useAppStore } from "../../stores/appStore"
import { respondModeSwitch } from "../../lib/tauri"

/** Docked chip shown above the composer when the engine proposes a
 * composer-mode switch (`ModeSwitchProposed`). Shows a countdown, lets the
 * user cancel, and auto-accepts when the veto window expires. */
export const ModeSwitchChip = () => {
  const pending = useAppStore((s) => s.pendingModeSwitch)
  const clearPending = useAppStore((s) => s.setPendingModeSwitch)
  const [remaining, setRemaining] = useState(0)
  const resolvedRef = useRef(false)

  useEffect(() => {
    if (!pending) {
      resolvedRef.current = false
      return
    }
    resolvedRef.current = false

    const tick = () => {
      const left = Math.max(0, pending.deadlineMs - Date.now())
      setRemaining(left)
      if (left <= 0 && !resolvedRef.current) {
        resolvedRef.current = true
        clearPending(null)
        void respondModeSwitch(pending.sessionId, pending.id, true).catch(() => {
          // Engine may have already auto-applied — safe no-op.
        })
      }
    }
    tick()
    const id = setInterval(tick, 100)
    return () => clearInterval(id)
  }, [pending, clearPending])

  if (!pending) return null

  const secs = Math.ceil(remaining / 1000)

  const handleCancel = () => {
    if (resolvedRef.current) return
    resolvedRef.current = true
    clearPending(null)
    void respondModeSwitch(pending.sessionId, pending.id, false).catch(() => {})
  }

  const modeLabel = pending.mode.charAt(0).toUpperCase() + pending.mode.slice(1)

  return (
    <div
      role="status"
      aria-live="polite"
      className="flex items-center gap-2 rounded-md border border-stroke-2 bg-fill-2 px-3 py-1.5 text-sm"
    >
      <Shuffle className="h-3.5 w-3.5 shrink-0 text-icon-2" aria-hidden />
      <span className="flex-1 truncate text-ink-secondary">
        Switching to&nbsp;<strong className="text-ink">{modeLabel}</strong>
        {pending.reason ? (
          <span className="text-ink-muted"> · {pending.reason}</span>
        ) : null}
      </span>
      <span className="shrink-0 tabular-nums text-ink-muted" aria-label={`${secs} seconds`}>
        {secs}s
      </span>
      <Button
        variant="ghost"
        size="xs"
        onClick={handleCancel}
        aria-label="Cancel mode switch"
        title="Cancel mode switch"
        className="h-5 w-5 shrink-0 p-0 text-ink-muted hover:text-ink"
      >
        <X className="h-3 w-3" aria-hidden />
      </Button>
    </div>
  )
}
