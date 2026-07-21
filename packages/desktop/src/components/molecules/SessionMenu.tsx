import { useState } from "react"
import {
  Ellipsis,
  GitMerge,
  Pencil,
  Redo2,
  Trash2,
  Undo2,
  XCircle,
} from "lucide-react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  discardIsolatedSession,
  integrateSession,
  isIsolated,
  revertSnapshot,
  toInvokeError,
} from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

import { ConfirmDialog } from "./ConfirmDialog"
import { ErrorBanner } from "./ErrorBanner"
import { Input } from "@/components/ui/input"

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
    staleTime: 30_000,
    enabled: open,
  })

  const tipIndex = snapshots.length - 1
  const effectiveIndex = cursor < 0 ? tipIndex : cursor
  const canUndo = snapshots.length > 0 && effectiveIndex >= 0
  const canRedo = cursor >= 0 && cursor < tipIndex

  const handleOpenRename = () => {
    setOpen(false)
    setRenameValue(label)
    setRenameOpen(true)
  }

  const handleOpenDelete = () => {
    setOpen(false)
    setDeleteOpen(true)
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
    setOpen(false)
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
    setOpen(false)
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
    setOpen(false)
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
    setOpen(false)
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
        <DropdownMenuTrigger
          render={
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="Chat actions"
              className="size-6 text-muted-foreground hover:text-foreground aria-expanded:bg-fill-4 aria-expanded:text-foreground"
            />
          }
        >
          <Ellipsis className="size-3" aria-hidden />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" sideOffset={4} className="w-48">
          <DropdownMenuGroup>
            <DropdownMenuItem onClick={handleOpenRename}>
              <Pencil />
              Rename
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!canUndo || busy}
              onClick={() => void handleUndo()}
            >
              <Undo2 />
              Undo files
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!canRedo || busy}
              onClick={() => void handleRedo()}
            >
              <Redo2 />
              Redo files
            </DropdownMenuItem>
          </DropdownMenuGroup>
          {isolated ? (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuGroup>
                <DropdownMenuItem
                  disabled={busy}
                  onClick={() => void handleIntegrate()}
                >
                  <GitMerge />
                  Integrate workspace
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={busy}
                  onClick={() => void handleDiscard()}
                >
                  <XCircle />
                  Discard workspace
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </>
          ) : null}
          <DropdownMenuSeparator />
          <DropdownMenuGroup>
            <DropdownMenuItem
              variant="destructive"
              onClick={handleOpenDelete}
            >
              <Trash2 />
              Delete
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      {error ? (
        <div className="absolute right-0 top-full z-50 mt-1 w-56">
          <ErrorBanner
            message={error}
            onDismiss={() => setError(null)}
            className="py-1.5 text-xs shadow-md"
          />
        </div>
      ) : null}

      <ConfirmDialog
        open={renameOpen}
        title="Rename session"
        confirmLabel="Rename"
        isLoading={busy}
        onCancel={() => setRenameOpen(false)}
        onConfirm={() => void handleRename()}
      >
        <Input
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
