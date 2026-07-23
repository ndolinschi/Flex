import { useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ArrowRight } from "lucide-react"
import { ConfirmDialog, ErrorBanner } from "../../molecules"
import { revertSnapshot } from "../../../lib/tauri"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { cn } from "../../../lib/utils"
import { Button } from "@/components/ui/button"

export const CheckpointChip = ({
  sessionId,
  snapshotId,
  disabled,
}: {
  sessionId: string
  snapshotId: string
  disabled?: boolean
}) => {
  const [open, setOpen] = useState(false)
  const [isReverting, setIsReverting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const handleConfirm = async () => {
    setIsReverting(true)
    setError(null)
    try {
      await revertSnapshot(sessionId, snapshotId)
      invalidateGitQueries(queryClient, { sessionId })
      void queryClient.invalidateQueries({ queryKey: ["workspace-status"] })
      setOpen(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setIsReverting(false)
    }
  }

  return (
    <>
      <Button
        variant="ghost"
        disabled={disabled}
        onClick={() => setOpen(true)}
        className={cn(
          "group/checkpoint h-auto gap-1 px-0 py-0 font-normal text-sm leading-none text-ink-muted hover:bg-transparent",
          disabled ? "cursor-not-allowed" : "hover:text-ink-secondary",
        )}
      >
        <ArrowRight className="h-2.5 w-2.5" aria-hidden />
        <span>Restore Checkpoint</span>
      </Button>
      <ConfirmDialog
        open={open}
        title="Restore checkpoint?"
        description="Files will be reverted to their state at this point. The conversation is kept."
        confirmLabel="Restore"
        isLoading={isReverting}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (isReverting) return
          setOpen(false)
          setError(null)
        }}
      >
        {error ? <ErrorBanner message={error} /> : null}
      </ConfirmDialog>
    </>
  )
}

