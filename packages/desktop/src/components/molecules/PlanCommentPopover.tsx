import { useEffect, useState } from "react"
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

/** @deprecated Prefer `PlanSelectionAnchor` from `usePlanSelectionComment`. */
export type PlanCommentDraft = PlanSelectionAnchor

type PlanCommentPopoverProps = {
  draft: PlanSelectionAnchor | null
  onCancel: () => void
  onSave: (body: string) => void
  onSaveAndSend: (body: string) => void
  className?: string
}

/** Floating composer for a plan-text selection comment. */
export const PlanCommentPopover = ({
  draft,
  onCancel,
  onSave,
  onSaveAndSend,
  className,
}: PlanCommentPopoverProps) => {
  const [body, setBody] = useState("")

  useEffect(() => {
    setBody("")
  }, [draft])

  if (!draft) return null

  const trimmed = body.trim()
  const canSubmit = trimmed.length > 0

  return (
    <Popover
      open
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
    >
      {/* Virtual anchor at the selection point — public API passes coords, not a DOM node. */}
      <PopoverTrigger
        nativeButton={false}
        tabIndex={-1}
        render={
          <span
            aria-hidden
            className="pointer-events-none fixed size-0"
            style={{ left: draft.anchor.x, top: draft.anchor.y }}
          />
        }
      />
      <PopoverContent
        side="bottom"
        align="center"
        sideOffset={0}
        data-suppress-native-webview=""
        className={cn("w-72", className)}
      >
        <PopoverTitle className="sr-only">Comment on plan</PopoverTitle>
        <p className="line-clamp-3 border-l-2 border-accent/40 pl-2 text-xs italic text-muted-foreground">
          {draft.quote}
        </p>
        <Textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Add a comment…"
          rows={3}
          className="w-full text-sm"
          aria-label="Comment text"
        />
        <div className="flex flex-wrap items-center justify-end gap-1.5">
          <Button variant="ghost" size="sm" onClick={onCancel}>
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
