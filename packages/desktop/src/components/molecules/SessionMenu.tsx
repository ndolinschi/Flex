import { useState } from "react"
import {
  Ellipsis,
  GitMerge,
  Pencil,
  Redo2,
  Trash2,
  Undo2,
  XCircle,
} from "@/components/icons"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  discardIsolatedSession,
  integrateSession,
  isIsolated,
  revertSnapshot,
  toInvokeError,
} from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { IconButton, TextInput } from "../atoms"
import { ConfirmDialog } from "./ConfirmDialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type SessionMenuProps = {
  sessionId: string
  label: string
  onRename: (id: string, title: string) => Promise<void>
  onDelete: (id: string) => Promise<void>
}

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_SNAPSHOTS: string[] = []

/** Top-bar ellipsis menu with session actions (rename / delete / workspace / undo). */
export const SessionMenu = ({
  sessionId,
  label,
  onRename,
  onDelete,
}: SessionMenuProps) => {
  const [open, setOpen] = useState(false)
  const [renameOpen, setRenameOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [renameValue, setRenameValue] = useState(label)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const snapshots = useAppStore(
    (s) => s.snapshotsBySession[sessionId] ?? EMPTY_SNAPSHOTS,
  )
  const cursor = useAppStore((s) => s.snapshotIndexBySession[sessionId] ?? -1)
  const setSnapshotIndex = useAppStore((s) => s.setSnapshotIndex)

  const { data: isolated = false, refetch: refetchIsolated } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId),
    staleTime: 5_000,
  })

  const tipIndex = snapshots.length - 1
  const effectiveIndex = cursor < 0 ? tipIndex : cursor
  const canUndo = snapshots.length > 0 && effectiveIndex >= 0
  const canRedo = cursor >= 0 && cursor < tipIndex

  const handleOpenRename = () => {
    setRenameValue(label)
    setRenameOpen(true)
  }

  const handleRename = async () => {
    const title = renameValue.trim()
    if (!title) return
    setBusy(true)
    try {
      await onRename(sessionId, title)
      setRenameOpen(false)
    } finally {
      setBusy(false)
    }
  }

  const handleDelete = async () => {
    setBusy(true)
    try {
      await onDelete(sessionId)
      setDeleteOpen(false)
    } finally {
      setBusy(false)
    }
  }

  const handleIntegrate = async () => {
    setBusy(true)
    setError(null)
    try {
      await integrateSession(sessionId)
      await refetchIsolated()
      void queryClient.invalidateQueries({ queryKey: ["sessions"] })
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const handleDiscard = async () => {
    setBusy(true)
    setError(null)
    try {
      await discardIsolatedSession(sessionId)
      await refetchIsolated()
      void queryClient.invalidateQueries({ queryKey: ["sessions"] })
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const handleUndo = async () => {
    if (!canUndo) return
    const target = snapshots[effectiveIndex]
    setBusy(true)
    setError(null)
    try {
      await revertSnapshot(sessionId, target)
      setSnapshotIndex(sessionId, Math.max(effectiveIndex - 1, -1))
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const handleRedo = async () => {
    if (!canRedo) return
    const next = cursor + 1
    const target = snapshots[next]
    setBusy(true)
    setError(null)
    try {
      await revertSnapshot(sessionId, target)
      setSnapshotIndex(sessionId, next === tipIndex ? -1 : next)
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="relative">
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <IconButton
            label="Chat actions"
            quiet
            className={cn("h-6 w-6", open && "bg-fill-3 text-ink opacity-100")}
          >
            <Ellipsis className="h-3 w-3" aria-hidden />
          </IconButton>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          align="end"
          sideOffset={4}
          className="w-48 min-w-48 rounded-lg border border-stroke-2 bg-panel p-0.5 shadow-lg ring-0"
        >
          <DropdownMenuGroup>
            <DropdownMenuItem
              className="gap-2 px-2.5 py-1.5 text-base"
              onSelect={handleOpenRename}
            >
              <Pencil className="size-3.5 text-icon-3" aria-hidden />
              Rename
            </DropdownMenuItem>
            <DropdownMenuItem
              className="gap-2 px-2.5 py-1.5 text-base"
              disabled={!canUndo || busy}
              onSelect={() => void handleUndo()}
            >
              <Undo2 className="size-3.5 text-icon-3" aria-hidden />
              Undo files
            </DropdownMenuItem>
            <DropdownMenuItem
              className="gap-2 px-2.5 py-1.5 text-base"
              disabled={!canRedo || busy}
              onSelect={() => void handleRedo()}
            >
              <Redo2 className="size-3.5 text-icon-3" aria-hidden />
              Redo files
            </DropdownMenuItem>
          </DropdownMenuGroup>
          {isolated ? (
            <>
              <DropdownMenuSeparator className="mx-2 bg-stroke-3" />
              <DropdownMenuGroup>
                <DropdownMenuItem
                  className="gap-2 px-2.5 py-1.5 text-base"
                  disabled={busy}
                  onSelect={() => void handleIntegrate()}
                >
                  <GitMerge className="size-3.5 text-icon-3" aria-hidden />
                  Integrate workspace
                </DropdownMenuItem>
                <DropdownMenuItem
                  className="gap-2 px-2.5 py-1.5 text-base"
                  disabled={busy}
                  onSelect={() => void handleDiscard()}
                >
                  <XCircle className="size-3.5 text-icon-3" aria-hidden />
                  Discard workspace
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </>
          ) : null}
          <DropdownMenuSeparator className="mx-2 bg-stroke-3" />
          <DropdownMenuItem
            variant="destructive"
            className="gap-2 px-2.5 py-1.5 text-base"
            onSelect={() => setDeleteOpen(true)}
          >
            <Trash2 className="size-3.5" aria-hidden />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {error ? (
        <p className="absolute right-0 top-full z-50 mt-10 w-56 rounded-md bg-danger-subtle px-2 py-1 text-xs text-danger">
          {error}
        </p>
      ) : null}

      <ConfirmDialog
        open={renameOpen}
        title="Rename session"
        confirmLabel="Rename"
        isLoading={busy}
        onCancel={() => setRenameOpen(false)}
        onConfirm={() => void handleRename()}
      >
        <TextInput
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          aria-label="Session title"
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault()
              void handleRename()
            }
          }}
        />
      </ConfirmDialog>

      <ConfirmDialog
        open={deleteOpen}
        title="Delete session"
        description={`Delete "${label}"? This cannot be undone.`}
        confirmLabel="Delete"
        danger
        isLoading={busy}
        onCancel={() => setDeleteOpen(false)}
        onConfirm={() => void handleDelete()}
      />
    </div>
  )
}
