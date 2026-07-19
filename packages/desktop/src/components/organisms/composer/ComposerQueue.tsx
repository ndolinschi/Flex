import { Button } from "@/components/ui/button"
import { X } from "lucide-react"

type ComposerQueueProps = {
  items: string[]
  onSendNow: (index: number) => void
  onRemove: (index: number) => void
}

export const ComposerQueue = ({ items, onSendNow, onRemove }: ComposerQueueProps) => {
  if (items.length === 0) return null
  return (
    <div className="mx-auto mb-1.5 flex w-full max-w-[var(--content-rail)] flex-col gap-1">
      {items.map((item, index) => (
        <div
          key={`${index}-${item.slice(0, 24)}`}
          className="animate-tray-in flex items-center gap-2 rounded-md bg-fill-4 px-2.5 py-1.5 text-sm text-ink-secondary"
        >
          <span className="shrink-0 text-xs text-ink-faint">Queued</span>
          <span className="min-w-0 flex-1 truncate">{item}</span>
          <Button
            variant="link"
            size="xs"
            onClick={() => onSendNow(index)}
            className="h-auto shrink-0 p-0 text-accent hover:text-accent-hover"
          >
            Send now
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => onRemove(index)}
            aria-label="Remove queued message"
            className="shrink-0 text-ink-faint hover:bg-transparent hover:text-ink"
          >
            <X aria-hidden />
          </Button>
        </div>
      ))}
    </div>
  )
}
