import { ArrowUpIcon, SquareIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

type SendButtonProps = {
  isStreaming: boolean
  /** When streaming with draft text — show queue send instead of only stop. */
  canQueue?: boolean
  disabled?: boolean
  onSend: () => void
  onStop: () => void
}

/** Production send: h-6 w-6 rounded-full; bg-neutral (= base ink) + inverted fg. */
const armedClass = cn(
  "size-6 shrink-0 rounded-full border-0 bg-send text-send-fg shadow-none",
  "hover:opacity-90 active:translate-y-px",
  "transition-[opacity,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
)
const idleClass = cn(
  "size-6 shrink-0 rounded-full border-0 bg-send text-send-fg shadow-none",
  "opacity-35 hover:opacity-50",
  "disabled:pointer-events-none disabled:opacity-30",
  "transition-[opacity,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
)

/** 24px circle send / stop / queue — shadcn Button icon shell. */
export const SendButton = ({
  isStreaming,
  canQueue = false,
  disabled = false,
  onSend,
  onStop,
}: SendButtonProps) => {
  // Skip ButtonGroup: its shared-edge radius/border rules fight the
  // independent `rounded-full` h-6 circles (would flatten to joined pills).
  if (isStreaming && canQueue) {
    return (
      <div className="flex items-center gap-1.5">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onStop}
          aria-label="Stop generation"
          className={armedClass}
        >
          <SquareIcon className="size-2.5 fill-current" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onSend}
          aria-label="Queue message"
          className={armedClass}
        >
          <ArrowUpIcon className="size-3.5" strokeWidth={2.5} />
        </Button>
      </div>
    )
  }

  if (isStreaming) {
    return (
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        onClick={onStop}
        aria-label="Stop generation"
        className={armedClass}
      >
        <SquareIcon className="size-2.5 fill-current" />
      </Button>
    )
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      onClick={onSend}
      disabled={disabled}
      aria-label="Send message"
      className={disabled ? idleClass : armedClass}
    >
      <ArrowUpIcon className="size-3.5" strokeWidth={2.5} />
    </Button>
  )
}
