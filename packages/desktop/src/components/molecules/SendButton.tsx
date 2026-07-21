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

const circleClass = cn(
  "size-6 shrink-0 rounded-full bg-send text-send-fg opacity-80",
  "hover:opacity-100 hover:bg-send hover:text-send-fg",
)
const disabledCircleClass = cn(circleClass, "disabled:opacity-30")

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
          className={circleClass}
        >
          <SquareIcon className="size-2.5 fill-current" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onSend}
          aria-label="Queue message"
          className={circleClass}
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
        className={circleClass}
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
      className={disabledCircleClass}
    >
      <ArrowUpIcon className="size-3.5" strokeWidth={2.5} />
    </Button>
  )
}
