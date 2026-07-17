import type { ReactNode } from "react"
import { Button } from "../atoms"
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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

const footerClass =
  "mx-0 mb-0 mt-4 border-0 bg-transparent p-0 sm:justify-end"

const contentPlacement =
  "top-[10vh] max-w-[500px] translate-y-0 gap-0 sm:max-w-[500px]"

/** Controlled confirm/form modal.
 * Pure confirms (no children) use AlertDialog (`role=alertdialog`).
 * Forms keep Dialog so inputs stay valid dialog content.
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
  const handleOpenChange = (next: boolean) => {
    if (!next) onCancel()
  }

  const actions = (
    <>
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
    </>
  )

  if (!children) {
    return (
      <AlertDialog open={open} onOpenChange={handleOpenChange}>
        <AlertDialogContent
          data-suppress-native-webview=""
          className={cn(
            contentPlacement,
            /* Wider than nova default; top-biased placement */
            "data-[size=default]:max-w-[500px] data-[size=default]:sm:max-w-[500px]",
          )}
        >
          <AlertDialogHeader className="gap-1 text-left sm:group-data-[size=default]/alert-dialog-content:place-items-start sm:group-data-[size=default]/alert-dialog-content:text-left">
            <AlertDialogTitle className="text-base font-semibold text-ink">
              {title}
            </AlertDialogTitle>
            <AlertDialogDescription className="text-sm text-ink-muted">
              {description ?? title}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className={footerClass}>{actions}</AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        showCloseButton={false}
        data-suppress-native-webview=""
        className={cn(contentPlacement)}
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
        <div className="mt-3">{children}</div>
        <DialogFooter className={footerClass}>{actions}</DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
