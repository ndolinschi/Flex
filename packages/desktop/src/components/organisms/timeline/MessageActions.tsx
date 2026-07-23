import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Check, Copy } from "lucide-react"
import { cn, formatRelativeTime } from "../../../lib/utils"

export const MessageActions = ({
  text,
  tsMs,
  hideTimestamp = false,
  reveal = "hover",
  className,
}: {
  text: string
  tsMs: number
  hideTimestamp?: boolean
  reveal?: "hover" | "always"
  className?: string
}) => {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
    }
  }

  return (
    <div
      className={cn(
        "mt-1 flex h-5 items-center justify-end gap-0.5",
        reveal === "hover" &&
          "opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/row:opacity-100 group-focus-within/row:opacity-100",
        reveal === "always" && "mt-0",
        className,
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
        aria-label={copied ? "Copied" : "Copy message"}
        title={copied ? "Copied" : "Copy message"}
        onClick={(e) => {
          e.stopPropagation()
          void handleCopy()
        }}
        className={cn(
          "h-5 w-5 p-0 text-icon-2 hover:bg-bg-quaternary hover:text-icon-1",
        )}
      >
        {copied ? (
          <Check className="h-3.5 w-3.5 text-green" aria-hidden />
        ) : (
          <Copy className="h-3.5 w-3.5" strokeWidth={1.5} aria-hidden />
        )}
      </Button>
    </div>
  )
}
