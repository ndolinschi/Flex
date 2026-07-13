import { useEffect, useRef, useState } from "react"
import { Button, TextArea } from "../atoms"
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
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    setBody("")
    if (draft) {
      requestAnimationFrame(() => textareaRef.current?.focus())
    }
  }, [draft])

  useEffect(() => {
    if (!draft) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onCancel()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [draft, onCancel])

  if (!draft) return null

  const trimmed = body.trim()
  const canSubmit = trimmed.length > 0

  // Keep the popover inside the viewport.
  const left = Math.min(Math.max(8, draft.anchor.x), window.innerWidth - 320)
  const top = Math.min(Math.max(8, draft.anchor.y), window.innerHeight - 220)

  return (
    <div
      role="dialog"
      aria-label="Comment on plan"
      className={cn(
        "fixed z-50 w-72 rounded-md border border-stroke-2 bg-surface-1 p-3 shadow-lg",
        className,
      )}
      style={{ left, top }}
    >
      <p className="mb-2 line-clamp-3 border-l-2 border-accent/40 pl-2 text-xs italic text-ink-muted">
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
      <div className="mt-2 flex flex-wrap items-center justify-end gap-1.5">
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
    </div>
  )
}
