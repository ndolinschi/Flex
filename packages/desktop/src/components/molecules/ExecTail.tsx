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

const useExecErrorScan = (callId: string) =>
  useSyncExternalStore(
    (onChange) => subscribeExecErrorScan(callId, onChange),
    () => getExecErrorScan(callId),
  )

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
      <Button variant="link" onClick={handleAskAgent} className="h-auto px-0 py-0">
        Ask Agent to fix
      </Button>
    </span>
  )
}

