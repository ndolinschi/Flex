import { useState, useSyncExternalStore } from "react"
import { ListEnd, Square } from "lucide-react"
import { backgroundKill } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { getExecTail, subscribeExecTail } from "../../lib/execTailBus"
import { IconButton } from "../atoms/IconButton"
import { ExecErrorAction, ExecTail } from "./ExecTail"
import type { ToolStepDetail } from "../../lib/toolPresentation"
import { parseExitMarker } from "../../lib/toolPresentation"

const useBackgroundExitState = (
  callId: string,
): { exited: true; code: number | null } | { exited: false } => {
  const tail = useSyncExternalStore(
    (onChange) => subscribeExecTail(callId, onChange),
    () => getExecTail(callId),
  )
  return parseExitMarker(tail)
}

/** Distinct feed row for a `Bash` call started with `run_in_background:
 * true` (see `isBackgroundBashCall`). Renders a subtle pulsing dot + Stop
 * button while running; once the engine's `[process exited...]` marker
 * appears in the tail (authoritative — see `parseExitMarker`), swaps to an
 * exited state showing the code when parseable. The persisted tail keeps
 * rendering underneath via the same `ExecTail` used for foreground shell
 * rows. */
export const BackgroundBashRow = ({ detail }: { detail: ToolStepDetail }) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const exitState = useBackgroundExitState(detail.id)
  const [stopping, setStopping] = useState(false)
  const [stopError, setStopError] = useState<string | null>(null)

  const exited = exitState.exited
  // Structured `running` from the start call's result is the fallback before
  // any tail has streamed in (e.g. preview mock with no exec_chunk yet);
  // the exit marker always wins once seen.
  const running = !exited && (detail.background?.initiallyRunning ?? detail.running)

  const handleStop = () => {
    if (!sessionId || !detail.background?.processId || stopping) return
    setStopping(true)
    setStopError(null)
    backgroundKill(sessionId, detail.background.processId)
      .catch((err) => setStopError(err instanceof Error ? err.message : String(err)))
      .finally(() => setStopping(false))
  }

  return (
    <li className="flex flex-col">
      <div className="flex min-h-6 items-center gap-1.5 text-[13px] leading-[1.5] text-ink-muted">
        <span className="flex h-3 w-3 shrink-0 items-center justify-center">
          <ListEnd className="h-3 w-3 text-ink-faint" aria-hidden />
        </span>
        {running ? (
          <span
            className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-green"
            aria-hidden
          />
        ) : null}
        <span className="min-w-0 shrink truncate text-[12px] [font-variant-numeric:tabular-nums] text-ink-secondary">
          {detail.label}
        </span>
        <span className="shrink-0 text-ink-faint">
          {exited
            ? exitState.code != null
              ? `exited (code ${exitState.code})`
              : "exited"
            : running
              ? "running"
              : null}
        </span>
        {running && detail.background?.processId ? (
          <IconButton
            label="Stop process"
            isLoading={stopping}
            onClick={handleStop}
            className="ml-auto h-5 w-5"
          >
            <Square className="h-3 w-3" aria-hidden />
          </IconButton>
        ) : null}
      </div>
      {stopError ? (
        <div className="ml-3.5 mt-0.5 text-[11px] text-danger">{stopError}</div>
      ) : null}
      <ExecTail callId={detail.id} muted={!running} />
      {exited ? (
        <ExecErrorAction
          callId={detail.id}
          command={detail.command ?? detail.label}
        />
      ) : null}
    </li>
  )
}

/** "Move to background" affordance for a running foreground shell row (see
 * `MOVE-TO-BACKGROUND`, `detail.canDemote`): sits next to the running
 * spinner, mirroring the reference design's inline row action. On click,
 * calls `backgroundDemote`; a `false` result (nothing to demote — the call
 * already finished, or the backend doesn't support it) is treated as a
 * silent no-op, not an error, since it's a benign race rather than a
 * failure. On success the row flips to the background presentation on its
 * own once the engine's demoted result lands (see `isDemotedBashCall`) — no
 * local "demoted" state to track here. */
