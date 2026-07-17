import { useEffect, useRef, useState } from "react"
import { Button, TextArea } from "../atoms"
import { cn } from "../../lib/utils"
import type { PlanSelectionAnchor } from "../../hooks/usePlanSelectionComment"
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from "@/components/ui/popover"

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
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const open = draft != null

  useEffect(() => {
    setBody("")
    if (draft) {
      requestAnimationFrame(() => textareaRef.current?.focus())
    }
  }, [draft])

  const trimmed = body.trim()
  const canSubmit = trimmed.length > 0

  // Keep the popover inside the viewport.
  const left = draft
    ? Math.min(Math.max(8, draft.anchor.x), window.innerWidth - 320)
    : 0
  const top = draft
    ? Math.min(Math.max(8, draft.anchor.y), window.innerHeight - 220)
    : 0

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
    >
      {draft ? (
        <PopoverAnchor asChild>
          <span
            aria-hidden
            className="pointer-events-none fixed size-0"
            style={{ left, top }}
          />
        </PopoverAnchor>
      ) : null}
      <PopoverContent
        side="bottom"
        align="start"
        sideOffset={4}
        data-suppress-native-webview=""
        onOpenAutoFocus={(e) => {
          e.preventDefault()
          textareaRef.current?.focus()
        }}
        className={cn(
          "w-72 gap-2 border-stroke-2 bg-surface-1 p-3 shadow-lg",
          className,
        )}
        aria-label="Comment on plan"
      >
        {draft ? (
          <>
            <p className="line-clamp-3 border-l-2 border-accent/40 pl-2 text-xs italic text-ink-muted">
              {draft.quote}
            </p>
            <TextArea
              ref={textareaRef}
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
                variant="primary"
                size="sm"
                disabled={!canSubmit}
                onClick={() => canSubmit && onSaveAndSend(trimmed)}
              >
                Save & send
              </Button>
            </div>
          </>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}
