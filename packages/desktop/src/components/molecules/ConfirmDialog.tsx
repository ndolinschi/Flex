import { useEffect, useRef, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { Button } from "../atoms"
import { cn } from "../../lib/utils"

type ConfirmDialogProps = {
  open: boolean
  title: string
  description?: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
  isLoading?: boolean
  /** Extra disable for confirm (e.g. empty required fields). */
  confirmDisabled?: boolean
  onConfirm: () => void
  onCancel: () => void
  children?: ReactNode
}

/** In-app modal shell for rename / delete (replaces window.prompt/confirm).
 * Portaled to `document.body` so virtualized timeline rows (and other
 * remounting parents) cannot unmount an open dialog mid-stream. */
export const ConfirmDialog = ({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  danger = false,
  isLoading = false,
  confirmDisabled = false,
  onConfirm,
  onCancel,
  children,
}: ConfirmDialogProps) => {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onCancel()
      }
    }

    document.addEventListener("keydown", handleKey)
    const el = panelRef.current?.querySelector<HTMLElement>(
      "input, textarea, button:not([disabled])",
    )
    el?.focus()

    return () => document.removeEventListener("keydown", handleKey)
  }, [open, onCancel])

  if (!open) return null

  return createPortal(
    <div
      className="fixed inset-0 z-[300] flex items-start justify-center bg-black/20 pt-[10vh] animate-backdrop-in"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel()
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        className={cn(
          "w-full max-w-[500px] rounded-xl border border-stroke-2 bg-panel p-4 shadow-lg",
          "animate-modal-in",
        )}
      >
        <h2
          id="confirm-dialog-title"
          className="text-base font-semibold text-ink"
        >
          {title}
        </h2>
        {description ? (
          <p className="mt-1 text-sm text-ink-muted">{description}</p>
        ) : null}
        {children ? <div className="mt-3">{children}</div> : null}
        <div className="mt-4 flex justify-end gap-2">
          <Button
            size="sm"
            variant="secondary"
            disabled={isLoading}
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          <Button
            size="sm"
            variant={danger ? "danger" : "primary"}
            isLoading={isLoading}
            disabled={confirmDisabled || isLoading}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  )
}
