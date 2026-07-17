import { ArrowUp, Square } from "@/components/icons"
import { Button } from "../atoms"
import { cn } from "../../lib/utils"

type SendButtonProps = {
  isStreaming: boolean
  /** When streaming with draft text — show queue send instead of only stop. */
  canQueue?: boolean
  disabled?: boolean
  onSend: () => void
  onStop: () => void
}

const circleClass = cn(
  "h-6 w-6 shrink-0 rounded-full border-0 bg-send p-0 text-send-fg opacity-80",
  "hover:bg-send hover:opacity-100 hover:text-send-fg",
  "disabled:pointer-events-none disabled:opacity-30",
)

/** 24px circle send / stop / queue — matches Plus / Bypass composer controls. */
export const SendButton = ({
  isStreaming,
  canQueue = false,
  disabled = false,
  onSend,
  onStop,
}: SendButtonProps) => {
  if (isStreaming && canQueue) {
    return (
      <div className="flex items-center gap-1.5">
        <Button
          variant="ghost"
          size="sm"
          aria-label="Stop generation"
          className={circleClass}
          onClick={onStop}
        >
          <Square className="h-2.5 w-2.5 fill-current" aria-hidden />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          aria-label="Queue message"
          className={circleClass}
          onClick={onSend}
        >
          <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} aria-hidden />
        </Button>
      </div>
    )
  }

  if (isStreaming) {
    return (
      <Button
        variant="ghost"
        size="sm"
        aria-label="Stop generation"
        className={circleClass}
        onClick={onStop}
      >
        <Square className="h-2.5 w-2.5 fill-current" aria-hidden />
      </Button>
    )
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      aria-label="Send message"
      disabled={disabled}
      className={circleClass}
      onClick={onSend}
    >
      <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} aria-hidden />
    </Button>
  )
}
