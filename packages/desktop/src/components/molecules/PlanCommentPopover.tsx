import { useEffect, useState } from "react"
import { MessageSquareText } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "../../lib/utils"
import type { PlanSelectionAnchor } from "../../hooks/usePlanSelectionComment"

export type PlanCommentDraft = PlanSelectionAnchor

type PlanCommentPopoverProps = {
  selection: PlanSelectionAnchor | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onSave: (body: string) => void
  onSaveAndSend: (body: string) => void
  className?: string
}

export const PlanCommentPopover = ({
  selection,
  open,
  onOpenChange,
  onSave,
  onSaveAndSend,
  className,
}: PlanCommentPopoverProps) => {
  const [body, setBody] = useState("")

  useEffect(() => {
    if (open) setBody("")
  }, [open, selection?.startOffset, selection?.endOffset])

  if (!selection) return null

  const left = Math.min(
    Math.max(8, selection.anchor.x - 44),
    window.innerWidth - 100,
  )
  const top = Math.min(
    Math.max(8, selection.anchor.y),
    window.innerHeight - 40,
  )

  const trimmed = body.trim()
  const canSubmit = trimmed.length > 0

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <div
        data-suppress-native-webview=""
        className="fixed z-[var(--z-overlay)]"
        style={{ left, top }}
      >
        <PopoverTrigger
          render={
            <Button
              variant="default"
              size="sm"
              aria-label="Comment on selection"
              onMouseDown={(e) => {
                e.preventDefault()
              }}
              className={cn(open && "pointer-events-none opacity-0")}
            />
          }
        >
          <MessageSquareText className="h-3.5 w-3.5" aria-hidden />
          Comment
        </PopoverTrigger>
      </div>
      <PopoverContent
        side="bottom"
        align="center"
        sideOffset={8}
        data-suppress-native-webview=""
        className={cn("w-72", className)}
      >
        <PopoverTitle className="sr-only">Comment on plan</PopoverTitle>
        <p className="line-clamp-3 border-l-2 border-accent/40 pl-2 text-xs italic text-ink-muted">
          {selection.quote}
        </p>
        <Textarea
          autoFocus
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Add a comment…"
          rows={3}
          className="w-full text-sm"
          aria-label="Comment text"
        />
        <div className="flex flex-wrap items-center justify-end gap-1.5">
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!canSubmit}
            onClick={() => canSubmit && onSave(trimmed)}
          >
            Save
          </Button>
          <Button
            variant="default"
            size="sm"
            disabled={!canSubmit}
            onClick={() => canSubmit && onSaveAndSend(trimmed)}
          >
            Save &amp; send
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  )
}
