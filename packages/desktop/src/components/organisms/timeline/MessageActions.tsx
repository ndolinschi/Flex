import { useState } from "react"
import { Check, Copy, ThumbsDown, ThumbsUp } from "lucide-react"
import { IconButton } from "../../atoms"
import { useAppStore } from "../../../stores/appStore"
import { cn, formatRelativeTime } from "../../../lib/utils"

export const MessageActions = ({
  text,
  tsMs,
  messageId,
}: {
  text: string
  tsMs: number
  /** Assistant messages only — enables the thumbs-up/down feedback buttons. */
  messageId?: string
}) => {
  const [copied, setCopied] = useState(false)
  const feedback = useAppStore((s) =>
    messageId ? s.messageFeedback[messageId] : undefined,
  )
  const setMessageFeedback = useAppStore((s) => s.setMessageFeedback)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // ignore
    }
  }

  const toggleFeedback = (value: "up" | "down") => {
    if (!messageId) return
    setMessageFeedback(messageId, feedback === value ? null : value)
  }

  return (
    <div
      className={cn(
        // Always visible (not hover-reveal) — space is reserved up front so
        // the row never shifts when it mounts.
        "mt-1 flex h-7 items-center justify-start gap-0.5",
      )}
    >
      <span className="px-1 text-sm text-ink-faint transition-colors duration-[var(--duration-fast)] group-hover/row:text-ink-muted">
        {formatRelativeTime(tsMs)}
      </span>
      <IconButton
        label={copied ? "Copied" : "Copy message"}
        className="h-6 w-6"
        onClick={() => void handleCopy()}
      >
        {copied ? (
          <Check className="h-3 w-3 text-green" aria-hidden />
        ) : (
          <Copy className="h-3 w-3" aria-hidden />
        )}
      </IconButton>
      {messageId ? (
        <>
          <IconButton
            label={feedback === "up" ? "Remove helpful feedback" : "Mark helpful"}
            className="h-6 w-6"
            onClick={() => toggleFeedback("up")}
          >
            <ThumbsUp
              className={cn(
                "h-3 w-3",
                feedback === "up" ? "text-green" : "text-ink-faint",
              )}
              aria-hidden
            />
          </IconButton>
          <IconButton
            label={feedback === "down" ? "Remove unhelpful feedback" : "Mark unhelpful"}
            className="h-6 w-6"
            onClick={() => toggleFeedback("down")}
          >
            <ThumbsDown
              className={cn(
                "h-3 w-3",
                feedback === "down" ? "text-red" : "text-ink-faint",
              )}
              aria-hidden
            />
          </IconButton>
        </>
      ) : null}
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
