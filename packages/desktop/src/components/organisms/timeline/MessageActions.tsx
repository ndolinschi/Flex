import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Check, Copy } from "lucide-react"
import { cn, formatRelativeTime } from "../../../lib/utils"

export const MessageActions = ({
  text,
  tsMs,
  hideTimestamp = false,
}: {
  text: string
  tsMs: number
  /** Suppress the relative-time label when the row's turn footer (see
   * `buildDisplayItems`'s `footer` attachment / `TurnFooter`) already renders
   * its own "just now"-style timestamp directly below this row — otherwise
   * the two stack and show the identical relative time twice. The copy
   * button stays either way: this copies `text` (the message content), while
   * the footer's copy button copies the whole turn's payload
   * (`buildTurnCopyText`) — different payloads, so both affordances earn
   * their place. */
  hideTimestamp?: boolean
}) => {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // ignore
    }
  }

  return (
    <div
      className={cn(
        // Always visible (not hover-reveal) — space is reserved up front so
        // the row never shifts when it mounts.
        "mt-1 flex h-7 items-center justify-start gap-0.5",
      )}
    >
      {hideTimestamp ? null : (
        <span className="px-1 text-sm text-ink-faint transition-colors duration-[var(--duration-fast)] group-hover/row:text-ink-muted">
          {formatRelativeTime(tsMs)}
        </span>
      )}
      <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={copied ? "Copied" : "Copy message"} title={copied ? "Copied" : "Copy message"}
      onClick={() => void handleCopy()}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
      )}
    >
      {copied ? (
          <Check className="h-3 w-3 text-green" aria-hidden />
        ) : (
          <Copy className="h-3 w-3" aria-hidden />
        )}
    </Button>
    </div>
  )
}

/**
 * End-of-turn footer: renders once, after the LAST rendered item of a
 * completed agent turn (see `buildDisplayItems`'s `footer` attachment) —
 * timestamp + duration + a "Copy response" button whose payload is the full
 * text the agent produced that turn (assistant text plus a plain-text list
 * of tool actions, already assembled by `buildTurnCopyText`). Absent while
 * the turn is still streaming; renders for historical/replayed turns too,
 * since it's derived purely from materialized rows.
 */
