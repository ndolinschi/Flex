import { useEffect, useRef, useState } from "react"
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
import { cn } from "../../lib/utils"
import { IconButton, TextInput } from "../atoms"
import { ConfirmDialog } from "./ConfirmDialog"

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
  const rootRef = useRef<HTMLDivElement>(null)
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

  useEffect(() => {
    const handlePointer = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener("mousedown", handlePointer)
    return () => document.removeEventListener("mousedown", handlePointer)
  }, [])

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

  const itemClass = cn(
    "flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-base",
    "text-ink-secondary transition-colors hover:bg-fill-3 hover:text-ink",
    "disabled:pointer-events-none disabled:opacity-40",
  )

  return (
    <div ref={rootRef} className="relative">
      <IconButton
        label="Chat actions"
        onClick={() => setOpen((v) => !v)}
        quiet
        className={cn("h-6 w-6", open && "bg-fill-3 text-ink opacity-100")}
      >
        <Ellipsis className="h-3 w-3" aria-hidden />
      </IconButton>

      {open ? (
        <div
          role="menu"
          className={cn(
            "absolute right-0 top-full z-50 mt-1 w-48 overflow-hidden rounded-lg",
            "border border-stroke-2 bg-panel py-0.5 shadow-lg animate-tray-in",
          )}
        >
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={handleOpenRename}
          >
            <Pencil className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Rename
          </button>
          <button
            type="button"
            role="menuitem"
            disabled={!canUndo || busy}
            className={itemClass}
            onClick={() => void handleUndo()}
          >
            <Undo2 className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Undo files
          </button>
          <button
            type="button"
            role="menuitem"
            disabled={!canRedo || busy}
            className={itemClass}
            onClick={() => void handleRedo()}
          >
            <Redo2 className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Redo files
          </button>
          {isolated ? (
            <>
              <div className="mx-2 my-0.5 border-t border-stroke-3" />
              <button
                type="button"
                role="menuitem"
                disabled={busy}
                className={itemClass}
                onClick={() => void handleIntegrate()}
              >
                <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Integrate workspace
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={busy}
                className={itemClass}
                onClick={() => void handleDiscard()}
              >
                <XCircle className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Discard workspace
              </button>
            </>
          ) : null}
          <div className="mx-2 my-0.5 border-t border-stroke-3" />
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={handleOpenDelete}
          >
            <Trash2 className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Delete
          </button>
        </div>
      ) : null}

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
