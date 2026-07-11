import { ArrowUp, Square } from "lucide-react"
import { cn } from "../../lib/utils"

type SendButtonProps = {
  isStreaming: boolean
  /** When streaming with draft text — show queue send instead of only stop. */
  canQueue?: boolean
  disabled?: boolean
  onSend: () => void
  onStop: () => void
}

/** 24px circle send / stop / queue — uses theme send tokens. */
export const SendButton = ({
  isStreaming,
  canQueue = false,
  disabled = false,
  onSend,
  onStop,
}: SendButtonProps) => {
  const circleClass = cn(
    "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full",
    "bg-send text-send-fg opacity-80",
    "transition-opacity duration-[var(--duration-fast)] ease-[var(--easing-default)]",
    "hover:opacity-100",
  )

  if (isStreaming && canQueue) {
    return (
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={onStop}
          aria-label="Stop generation"
          className={circleClass}
        >
          <Square className="h-2 w-2 fill-current" aria-hidden />
        </button>
        <button
          type="button"
          onClick={onSend}
          aria-label="Queue message"
          className={circleClass}
        >
          <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} aria-hidden />
        </button>
      </div>
    )
  }

  if (isStreaming) {
    return (
      <button
        type="button"
        onClick={onStop}
        aria-label="Stop generation"
        className={circleClass}
      >
        <Square className="h-2 w-2 fill-current" aria-hidden />
      </button>
    )
  }

  return (
    <button
      type="button"
      onClick={onSend}
      disabled={disabled}
      aria-label="Send message"
      className={cn(circleClass, "disabled:opacity-30 disabled:pointer-events-none")}
    >
      <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} aria-hidden />
    </button>
  )
}
