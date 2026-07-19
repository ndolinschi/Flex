import type { ReactNode } from "react"
import { Spinner } from "@/components/ui/spinner"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { cn } from "../../lib/utils"
import { TriangleAlertIcon } from "lucide-react"

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

/** In-app confirm / short-form modal on shadcn Base UI `AlertDialog`.
 * Controlled `open` — parent owns state; Esc / Cancel call `onCancel`.
 * Outside click does not dismiss (Alert Dialog default). */
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
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
    >
      <AlertDialogContent
        size="sm"
        className={cn(
          // Forms (rename, Create PR) need more than the default sm width.
          children && "max-w-[min(100%,32rem)] sm:max-w-lg",
        )}
      >
        <AlertDialogHeader>
          {danger ? (
            <AlertDialogMedia className="bg-destructive/10 text-destructive dark:bg-destructive/20 dark:text-destructive">
              <TriangleAlertIcon />
            </AlertDialogMedia>
          ) : null}
          <AlertDialogTitle>{title}</AlertDialogTitle>
          {description ? (
            <AlertDialogDescription>{description}</AlertDialogDescription>
          ) : children ? (
            <AlertDialogDescription className="sr-only">
              {title}
            </AlertDialogDescription>
          ) : (
            <AlertDialogDescription className="sr-only">
              Confirm this action.
            </AlertDialogDescription>
          )}
        </AlertDialogHeader>
        {children ? <div className="flex flex-col gap-3">{children}</div> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isLoading}>{cancelLabel}</AlertDialogCancel>
          <AlertDialogAction
            variant={danger ? "destructive" : "default"}
            disabled={confirmDisabled || isLoading}
            onClick={(e) => {
              // Action is a plain Button (not Close) — keep dialog open until
              // the parent sets `open={false}` after the async work finishes.
              e.preventDefault()
              if (confirmDisabled || isLoading) return
              onConfirm()
            }}
          >
            {isLoading ? <Spinner data-icon="inline-start" /> : null}
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
