import { useEffect, useMemo, useState, type MouseEvent } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  FileCode2,
  FilePlus,
  Pencil,
  Search,
  Trash2,
} from "lucide-react"
import {
  createTextFile,
  deletePath,
  listFiles,
  renamePath,
  toInvokeError,
} from "../../../lib/tauri"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { basename, cn, fileIconForPath } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { IconButton, Spinner, TextInput } from "../../atoms"
import { ConfirmDialog, ContextMenu, type ContextMenuItem } from "../../molecules"

type FileExplorerProps = {
  sessionId: string
  sessionKey: string
  cwd: string
  /** Called with a repo-relative file path (never a directory). */
  onOpenFile: (path: string) => void
}

type DialogState =
  | { kind: "create" }
  | { kind: "rename"; path: string }
  | { kind: "delete"; path: string }
  | null

const dirPrefix = (path: string): string => {
  const i = path.lastIndexOf("/")
  return i >= 0 ? path.slice(0, i + 1) : ""
}

const isValidRelativeFilePath = (path: string): boolean => {
  const trimmed = path.trim().replace(/\\/g, "/")
  if (!trimmed || trimmed.endsWith("/")) return false
  if (trimmed.startsWith("/") || trimmed.includes("..")) return false
  return true
}

const isValidBasename = (name: string): boolean => {
  const trimmed = name.trim()
  if (!trimmed) return false
  if (trimmed.includes("/") || trimmed.includes("\\") || trimmed.includes("..")) {
    return false
  }
  return true
}

/** Lightweight workspace file browser for the Files right-panel tab.
 * Same `list_files` IPC as composer `@` — project files only, gitignore +
 * hard-skip of `node_modules` / build outputs. Supports create / rename /
 * delete via header button + right-click context menu. */
export const FileExplorer = ({
  sessionId,
  sessionKey,
  cwd,
  onOpenFile,
}: FileExplorerProps) => {
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const closeWorkspaceFile = useAppStore((s) => s.closeWorkspaceFile)
  const renameWorkspaceFile = useAppStore((s) => s.renameWorkspaceFile)

  const [query, setQuery] = useState("")
  const trimmed = query.trim()
  const [debounced, setDebounced] = useState(trimmed)
  useEffect(() => {
    const handle = window.setTimeout(() => setDebounced(trimmed), 120)
    return () => window.clearTimeout(handle)
  }, [trimmed])

  const [menu, setMenu] = useState<{
    path: string
    x: number
    y: number
  } | null>(null)
  const [dialog, setDialog] = useState<DialogState>(null)
  const [draftPath, setDraftPath] = useState("")
  const [busy, setBusy] = useState(false)

  const { data: hits = [], isLoading, isFetching, refetch } = useQuery({
    queryKey: ["workspace-file-list", cwd, debounced],
    queryFn: () => listFiles(cwd, debounced),
    enabled: !!cwd,
    staleTime: 15_000,
  })

  const files = useMemo(
    () => hits.filter((h) => !h.is_dir && !h.path.endsWith("/")),
    [hits],
  )

  const refreshLists = async (paths: string[] = []) => {
    await refetch()
    for (const p of paths) {
      void queryClient.invalidateQueries({
        queryKey: ["workspace-file", sessionId, p],
      })
    }
    invalidateGitQueries(queryClient)
  }

  const openCreate = () => {
    setDraftPath("")
    setDialog({ kind: "create" })
  }

  const openRename = (path: string) => {
    setDraftPath(basename(path))
    setDialog({ kind: "rename", path })
  }

  const openDelete = (path: string) => {
    setDialog({ kind: "delete", path })
  }

  const handleContextMenu = (e: MouseEvent, path: string) => {
    e.preventDefault()
    e.stopPropagation()
    setMenu({ path, x: e.clientX, y: e.clientY })
  }

  const contextMenuItems: ContextMenuItem[] = menu
    ? [
        {
          type: "item",
          label: "Open",
          onSelect: () => onOpenFile(menu.path),
        },
        {
          type: "item",
          label: "Rename…",
          icon: Pencil,
          onSelect: () => openRename(menu.path),
        },
        { type: "separator" },
        {
          type: "item",
          label: "Delete…",
          icon: Trash2,
          danger: true,
          onSelect: () => openDelete(menu.path),
        },
      ]
    : []

  const confirmDisabled =
    dialog?.kind === "create"
      ? !isValidRelativeFilePath(draftPath)
      : dialog?.kind === "rename"
        ? !isValidBasename(draftPath)
        : false

  const handleConfirm = async () => {
    if (!dialog || busy) return
    setBusy(true)
    try {
      if (dialog.kind === "create") {
        const path = draftPath.trim().replace(/\\/g, "/")
        const created = await createTextFile(sessionId, path)
        await refreshLists([created])
        onOpenFile(created)
        pushToast(`Created ${created}`, "success")
      } else if (dialog.kind === "rename") {
        const name = draftPath.trim()
        const next = `${dirPrefix(dialog.path)}${name}`
        const renamed = await renamePath(sessionId, dialog.path, next)
        renameWorkspaceFile(sessionKey, dialog.path, renamed)
        await refreshLists([dialog.path, renamed])
        pushToast(`Renamed to ${basename(renamed)}`, "success")
      } else {
        await deletePath(sessionId, dialog.path)
        closeWorkspaceFile(sessionKey, dialog.path)
        await refreshLists([dialog.path])
        pushToast(`Deleted ${basename(dialog.path)}`, "success")
      }
      setDialog(null)
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2">
        <Search className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search files…"
          className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-faint"
          aria-label="Search workspace files"
        />
        {isFetching ? <Spinner size="sm" /> : null}
        <IconButton label="New file" onClick={openCreate} className="h-6 w-6">
          <FilePlus className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-1">
        {isLoading && files.length === 0 ? (
          <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
            <Spinner size="sm" />
            Loading…
          </div>
        ) : files.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
            <FileCode2 className="h-6 w-6 text-ink-faint" aria-hidden />
            <p className="text-sm text-ink-secondary">
              {trimmed ? "No matches" : "No files found"}
            </p>
            <p className="text-xs text-ink-muted">
              {trimmed
                ? "Try a different search."
                : "Create a file or open a project folder to browse."}
            </p>
            {!trimmed ? (
              <button
                type="button"
                onClick={openCreate}
                className="mt-1 text-xs text-accent hover:underline"
              >
                New file…
              </button>
            ) : null}
          </div>
        ) : (
          <ul className="flex flex-col" role="list">
            {files.map((hit) => {
              const Glyph = fileIconForPath(hit.path)
              return (
                <li key={hit.path}>
                  <button
                    type="button"
                    onClick={() => onOpenFile(hit.path)}
                    onContextMenu={(e) => handleContextMenu(e, hit.path)}
                    title={hit.path}
                    className={cn(
                      "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-sm",
                      "text-ink-secondary hover:bg-fill-4 hover:text-ink",
                    )}
                  >
                    <Glyph
                      className="h-3.5 w-3.5 shrink-0 text-ink-faint"
                      aria-hidden
                    />
                    <span className="min-w-0 flex-1 truncate">
                      <span className="text-ink-faint">
                        {hit.path.includes("/")
                          ? hit.path.slice(0, hit.path.lastIndexOf("/") + 1)
                          : ""}
                      </span>
                      {basename(hit.path)}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <ContextMenu
        position={menu ? { x: menu.x, y: menu.y } : null}
        items={contextMenuItems}
        onClose={() => setMenu(null)}
      />

      <ConfirmDialog
        open={dialog?.kind === "create"}
        title="New file"
        description="Repo-relative path (e.g. src/utils.ts). Parent folders are created automatically."
        confirmLabel="Create"
        confirmDisabled={confirmDisabled}
        isLoading={busy}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (!busy) setDialog(null)
        }}
      >
        <TextInput
          value={draftPath}
          onChange={(e) => setDraftPath(e.target.value)}
          placeholder="path/to/file.ts"
          aria-label="New file path"
          onKeyDown={(e) => {
            if (e.key === "Enter" && !confirmDisabled && !busy) {
              e.preventDefault()
              void handleConfirm()
            }
          }}
        />
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog?.kind === "rename"}
        title="Rename file"
        description={
          dialog?.kind === "rename"
            ? `Rename ${basename(dialog.path)}`
            : undefined
        }
        confirmLabel="Rename"
        confirmDisabled={confirmDisabled}
        isLoading={busy}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (!busy) setDialog(null)
        }}
      >
        <TextInput
          value={draftPath}
          onChange={(e) => setDraftPath(e.target.value)}
          placeholder="filename.ts"
          aria-label="New file name"
          onKeyDown={(e) => {
            if (e.key === "Enter" && !confirmDisabled && !busy) {
              e.preventDefault()
              void handleConfirm()
            }
          }}
        />
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog?.kind === "delete"}
        title="Delete file"
        description={
          dialog?.kind === "delete"
            ? `Permanently delete ${dialog.path}? This cannot be undone.`
            : undefined
        }
        confirmLabel="Delete"
        danger
        isLoading={busy}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (!busy) setDialog(null)
        }}
      />
    </div>
  )
}
