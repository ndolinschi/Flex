import type { ReactNode } from "react"
import { Button } from "../atoms"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"

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

/** Controlled confirm/form modal over shadcn Dialog.
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
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
    >
      <DialogContent
        showCloseButton={false}
        data-suppress-native-webview=""
        className={cn(
          /* Top-biased placement (was pt-[10vh]); wider than nova default */
          "top-[10vh] max-w-[500px] translate-y-0 gap-0 sm:max-w-[500px]",
        )}
      >
        <DialogHeader className="gap-1 text-left">
          <DialogTitle className="text-base font-semibold text-ink">
            {title}
          </DialogTitle>
          {description ? (
            <DialogDescription className="text-sm text-ink-muted">
              {description}
            </DialogDescription>
          ) : (
            <DialogDescription className="sr-only">{title}</DialogDescription>
          )}
        </DialogHeader>
        {children ? <div className="mt-3">{children}</div> : null}
        <DialogFooter className="mx-0 mb-0 mt-4 border-0 bg-transparent p-0 sm:justify-end">
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
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
