import { useSyncExternalStore } from "react"
import { getExecTail, subscribeExecTail } from "../../lib/execTailBus"
import { getExecErrorScan, subscribeExecErrorScan } from "../../lib/execErrorScan"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { Badge } from "../atoms/Badge"
import { Button } from "@/components/ui/button"

export const DiffBadge = ({
  added,
  removed,
}: {
  added?: number
  removed?: number
}) => {
  if (!added && !removed) return null
  return (
    <span className="inline-flex items-center gap-1 [font-variant-numeric:tabular-nums]">
      {added ? <span className="text-green">+{added}</span> : null}
      {removed ? <span className="text-red">-{removed}</span> : null}
    </span>
  )
}

/** Live mini-log: last ~5 lines of a command's buffered stdout/stderr,
 * rendered directly under its detail row (reference design: liveness feedback
 * for long-running commands, not just a spinner). Subscribes to execTailBus
 * via useSyncExternalStore so it updates as chunks stream in. Tails are no
 * longer cleared when the call completes (see execTailBus module doc), so
 * this keeps rendering after `running` flips to false — `muted` dims it
 * slightly further to read as "history" rather than a live feed. */
export const ExecTail = ({ callId, muted }: { callId: string; muted?: boolean }) => {
  const tail = useSyncExternalStore(
    (onChange) => subscribeExecTail(callId, onChange),
    () => getExecTail(callId),
  )
  if (!tail.trim()) return null
  const lines = tail.split("\n").filter((_, i, arr) => !(i === arr.length - 1 && arr[i] === ""))
  const lastLines = lines.slice(-5)
  return (
    <div
      className={cn(
        "ml-3.5 mt-0.5 max-h-[6.5em] overflow-hidden pl-2",
        muted && "opacity-60",
      )}
    >
      <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-[1.4] text-ink-faint">
        {lastLines.join("\n")}
      </pre>
    </div>
  )
}

/** Live error-scan read for a call's exec output (see `execErrorScan`).
 * Subscribes via `useSyncExternalStore` so the badge/action appears the
 * instant a matching line streams in, without waiting for the next
 * `tool_call_updated`. */
const useExecErrorScan = (callId: string) =>
  useSyncExternalStore(
    (onChange) => subscribeExecErrorScan(callId, onChange),
    () => getExecErrorScan(callId),
  )

/** "N errors in output" badge + "Ask Agent to fix" action for a COMPLETED
 * shell row whose exec output tripped the error scanner (see
 * `execErrorScan`). Deliberately not shown on running rows — the mini-log
 * tail already gives liveness feedback mid-run, and results aren't final yet
 * (see the call-site guard in `DetailRow`/`BackgroundRow`).
 *
 * "Ask Agent to fix" reuses the exact browser-error-page mechanism
 * (`setComposerDraft` + `flex:focus-composer` window event — see
 * `BrowserTab.handleAskAgent`) rather than inventing a second prefill path. */
export const ExecErrorAction = ({
  callId,
  command,
}: {
  callId: string
  command: string
}) => {
  const scan = useExecErrorScan(callId)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)

  if (!scan) return null

  const handleAskAgent = () => {
    const message = `The command \`${command}\` produced errors:\n\`\`\`\n${scan.lines.join("\n")}\n\`\`\`\nDiagnose and fix these errors.`
    setComposerDraft(message)
    window.dispatchEvent(new CustomEvent("flex:focus-composer"))
  }

  return (
    <span className="ml-3.5 mt-0.5 flex items-center gap-2">
      <Badge variant="danger">
        {scan.count} error{scan.count === 1 ? "" : "s"} in output
      </Badge>
      <Button
        variant="link"
        size="sm"
        onClick={handleAskAgent}
        className="h-auto px-0 py-0 text-sm text-accent"
      >
        Ask Agent to fix
      </Button>
    </span>
  )
}

/** Live "has this background process exited" read, derived from the same
 * exec-tail buffer `ExecTail` renders (see `parseExitMarker`) rather than
 * from `structured.running`, which only reflects the moment the start call
 * returned. Subscribes via `useSyncExternalStore` so it flips the instant the
 * exit-marker chunk lands, independent of any later `tool_call_updated`. */
