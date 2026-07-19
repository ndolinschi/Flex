import { useEffect, useMemo, useState, type MouseEvent } from "react"
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ChevronDown,
  FileCode2,
  FilePlus,
  Folder,
  FolderOpen,
  Pencil,
  Search,
  Trash2,
} from "lucide-react"
import {
  createTextFile,
  deletePath,
  gitStatusSinceBaseline,
  invalidateWorkspacePathCache,
  listDirChildren,
  listFiles,
  renamePath,
  resolveWorkspaceCwd,
  toInvokeError,
} from "../../../lib/tauri"
import { sortFileHits } from "../../../lib/fileTree"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import type { FileHit } from "../../../lib/types"
import { basename, cn, fileIconForPath } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { IconButton, Spinner, TextInput } from "../../atoms"
import { ConfirmDialog, ContextMenu, type ContextMenuItem } from "../../molecules"
import { STATUS_COLOR } from "./FileRow"

type FileExplorerProps = {
  sessionId: string
  sessionKey: string
  cwd: string
  /** Prefer when `cwd` is a missing isolated worktree (`session.base_cwd`). */
  fallbackCwd?: string
  /** Called with a repo-relative file path (never a directory). */
  onOpenFile: (path: string) => void
}

type DialogState =
  | { kind: "create"; prefix?: string }
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

const INDENT_PX = 12

/** Map git porcelain paths → status letter; also index dirty dir prefixes. */
const buildGitStatusIndex = (
  files: ReadonlyArray<{ path: string; status: string }> | undefined,
): {
  byPath: Map<string, string>
  dirtyDirs: Set<string>
} => {
  const byPath = new Map<string, string>()
  const dirtyDirs = new Set<string>()
  if (!files) return { byPath, dirtyDirs }
  for (const f of files) {
    const path = f.path.replace(/\\/g, "/")
    byPath.set(path, f.status)
    // Untracked dirs arrive with a trailing slash.
    if (path.endsWith("/")) {
      dirtyDirs.add(path.replace(/\/+$/, ""))
    }
    let rest = path.replace(/\/+$/, "")
    while (rest.includes("/")) {
      rest = rest.slice(0, rest.lastIndexOf("/"))
      if (!rest) break
      dirtyDirs.add(rest)
    }
  }
  return { byPath, dirtyDirs }
}

const gitStatusClass = (
  path: string,
  isDir: boolean,
  index: { byPath: Map<string, string>; dirtyDirs: Set<string> },
): string | undefined => {
  const normalized = path.replace(/\\/g, "/")
  if (!isDir) {
    const status = index.byPath.get(normalized)
    return status ? (STATUS_COLOR[status] ?? undefined) : undefined
  }
  const dirStatus =
    index.byPath.get(normalized) ??
    index.byPath.get(`${normalized}/`) ??
    (index.dirtyDirs.has(normalized) ? "M" : undefined)
  return dirStatus ? (STATUS_COLOR[dirStatus] ?? undefined) : undefined
}

type TreeBranchProps = {
  cwd: string
  fallbackCwd?: string
  dirPath: string
  depth: number
  expanded: Set<string>
  gitIndex: { byPath: Map<string, string>; dirtyDirs: Set<string> }
  onToggle: (dirPath: string) => void
  onOpenFile: (path: string) => void
  onContextMenu: (e: MouseEvent, hit: FileHit) => void
}

/** One directory level — loads children on demand when expanded (or root). */
const TreeBranch = ({
  cwd,
  fallbackCwd,
  dirPath,
  depth,
  expanded,
  gitIndex,
  onToggle,
  onOpenFile,
  onContextMenu,
}: TreeBranchProps) => {
  const isRoot = dirPath === ""
  const shouldLoad = isRoot || expanded.has(dirPath)

  const { data: children = [], isLoading, isFetching } = useQuery({
    queryKey: ["workspace-dir-children", cwd, fallbackCwd ?? "", dirPath],
    queryFn: () => listDirChildren(cwd, dirPath, fallbackCwd),
    enabled: !!cwd && shouldLoad,
    staleTime: 15_000,
  })

  // Hide cmd.exe artifacts like a literal `$null` file created when a
  // PowerShell redirect (`> $null`) was run under `cmd /C`.
  const sorted = useMemo(
    () => sortFileHits(children.filter((h) => h.name !== "$null")),
    [children],
  )

  if (!shouldLoad) return null

  if (isLoading && sorted.length === 0) {
    return (
      <div
        className="flex items-center gap-2 py-1 text-xs text-ink-muted"
        style={{ paddingLeft: 8 + depth * INDENT_PX }}
      >
        <Spinner size="sm" />
        Loading…
      </div>
    )
  }

  if (sorted.length === 0) {
    if (isRoot) {
      return (
        <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
          <Folder className="h-6 w-6 text-ink-faint" aria-hidden />
          <p className="text-sm text-ink-secondary">This folder is empty</p>
          <p className="text-xs text-ink-muted">Create a file to get started.</p>
        </div>
      )
    }
    return (
      <div
        className="py-1 text-xs text-ink-faint"
        style={{ paddingLeft: 8 + (depth + 1) * INDENT_PX }}
      >
        Empty
      </div>
    )
  }

  return (
    <ul className="flex flex-col" role="list">
      {sorted.map((hit) => {
        const isDir = !!hit.isDir
        const isOpen = isDir && expanded.has(hit.path)
        const Glyph = isDir
          ? isOpen
            ? FolderOpen
            : Folder
          : fileIconForPath(hit.path)
        const statusClass = gitStatusClass(hit.path, isDir, gitIndex)
        return (
          <li key={hit.path}>
            <button
              type="button"
              onClick={() => {
                if (isDir) onToggle(hit.path)
                else onOpenFile(hit.path)
              }}
              onContextMenu={(e) => onContextMenu(e, hit)}
              title={hit.path}
              aria-expanded={isDir ? isOpen : undefined}
              className={cn(
                "flex h-7 w-full items-center gap-1 rounded-md pr-2 text-left text-sm",
                "hover:bg-fill-4",
                statusClass ?? "text-ink-secondary hover:text-ink",
              )}
              style={{ paddingLeft: 8 + depth * INDENT_PX }}
            >
              <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
                {isDir ? (
                  <ChevronDown
                    className={cn(
                      "h-3 w-3 text-icon-3 opacity-70 transition-transform",
                      !isOpen && "-rotate-90",
                    )}
                    aria-hidden
                  />
                ) : (
                  <span className="w-3" />
                )}
              </span>
              <Glyph
                className={cn(
                  "h-3.5 w-3.5 shrink-0",
                  statusClass ?? "text-ink-faint",
                )}
                aria-hidden
              />
              <span className="min-w-0 flex-1 truncate">{hit.name}</span>
              {isDir && isFetching && isOpen ? (
                <Spinner size="sm" />
              ) : null}
            </button>
            {isDir && isOpen ? (
              <TreeBranch
                cwd={cwd}
                fallbackCwd={fallbackCwd}
                dirPath={hit.path}
                depth={depth + 1}
                expanded={expanded}
                gitIndex={gitIndex}
                onToggle={onToggle}
                onOpenFile={onOpenFile}
                onContextMenu={onContextMenu}
              />
            ) : null}
          </li>
        )
      })}
    </ul>
  )
}

/** VS Code–style workspace browser for the Files right-panel tab.
 * Browse mode: expandable folder tree via `list_dir_children`.
 * Search mode: flat `list_files` results (files + folders).
 * Create / rename / delete via header button + right-click context menu. */
export const FileExplorer = ({
  sessionId,
  sessionKey,
  cwd,
  fallbackCwd,
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

  const searching = debounced.length > 0
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set())

  // Fresh tree when the workspace root changes.
  useEffect(() => {
    setExpanded(new Set())
  }, [cwd, fallbackCwd])

  const { data: resolvedCwd, isLoading: resolvingCwd } = useQuery({
    queryKey: ["workspace-cwd-resolve", cwd, fallbackCwd ?? ""],
    queryFn: () => resolveWorkspaceCwd(cwd, fallbackCwd),
    enabled: !!cwd,
    staleTime: 30_000,
  })

  const [menu, setMenu] = useState<{
    hit: FileHit
    x: number
    y: number
  } | null>(null)
  const [dialog, setDialog] = useState<DialogState>(null)
  const [draftPath, setDraftPath] = useState("")
  const [busy, setBusy] = useState(false)

  const {
    data: searchHits = [],
    isLoading: searchLoading,
    isFetching: searchFetching,
  } = useQuery({
    queryKey: ["workspace-file-list", cwd, fallbackCwd ?? "", debounced],
    queryFn: () => listFiles(cwd, debounced, true, fallbackCwd),
    enabled: !!cwd && !!resolvedCwd && searching,
    staleTime: 15_000,
    placeholderData: keepPreviousData,
  })

  // Shared with Changes / sidebar — color dirty files in the tree.
  const { data: gitSummary } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId),
    enabled: !!cwd && !!sessionId,
    staleTime: 15_000,
  })
  const gitIndex = useMemo(
    () => buildGitStatusIndex(gitSummary?.files),
    [gitSummary?.files],
  )

  const searchRows = useMemo(
    () => sortFileHits(searchHits.filter((h) => h.name !== "$null")),
    [searchHits],
  )

  const refreshLists = async (paths: string[] = []) => {
    await invalidateWorkspacePathCache(cwd)
    await queryClient.invalidateQueries({
      queryKey: ["workspace-dir-children", cwd],
    })
    await queryClient.invalidateQueries({
      queryKey: ["workspace-file-list", cwd],
    })
    for (const p of paths) {
      void queryClient.invalidateQueries({
        queryKey: ["workspace-file", sessionId, p],
      })
    }
    invalidateGitQueries(queryClient)
  }

  const toggleDir = (dirPath: string) => {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(dirPath)) next.delete(dirPath)
      else next.add(dirPath)
      return next
    })
  }

  const openCreate = (prefix = "") => {
    setDraftPath(prefix)
    setDialog({ kind: "create", prefix })
  }

  const openRename = (path: string) => {
    setDraftPath(basename(path))
    setDialog({ kind: "rename", path })
  }

  const openDelete = (path: string) => {
    setDialog({ kind: "delete", path })
  }

  const handleContextMenu = (e: MouseEvent, hit: FileHit) => {
    e.preventDefault()
    e.stopPropagation()
    setMenu({ hit, x: e.clientX, y: e.clientY })
  }

  const menuHit = menu?.hit
  const contextMenuItems: ContextMenuItem[] = menuHit
    ? menuHit.isDir
      ? [
          {
            type: "item",
            label: expanded.has(menuHit.path) ? "Collapse" : "Expand",
            onSelect: () => toggleDir(menuHit.path),
          },
          {
            type: "item",
            label: "New file here…",
            icon: FilePlus,
            onSelect: () => openCreate(`${menuHit.path}/`),
          },
        ]
      : [
          {
            type: "item",
            label: "Open",
            onSelect: () => onOpenFile(menuHit.path),
          },
          {
            type: "item",
            label: "Rename…",
            icon: Pencil,
            onSelect: () => openRename(menuHit.path),
          },
          { type: "separator" },
          {
            type: "item",
            label: "Delete…",
            icon: Trash2,
            danger: true,
            onSelect: () => openDelete(menuHit.path),
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
        // Expand parent so the new file is visible in the tree.
        const parent = dirPrefix(created).replace(/\/$/, "")
        if (parent) {
          setExpanded((prev) => new Set(prev).add(parent))
        }
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
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
        <Search className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search files…"
          className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-faint"
          aria-label="Search workspace files"
        />
        {searchFetching ? <Spinner size="sm" /> : null}
        <IconButton label="New file" onClick={() => openCreate()} className="h-6 w-6">
          <FilePlus className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2.5 py-1">
        {searching ? (
          searchLoading && searchRows.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
              <Spinner size="sm" />
              Searching…
            </div>
          ) : searchRows.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
              <FileCode2 className="h-6 w-6 text-ink-faint" aria-hidden />
              <p className="text-sm text-ink-secondary">No matches</p>
              <p className="text-xs text-ink-muted">Try a different search.</p>
            </div>
          ) : (
            <ul className="flex flex-col" role="list">
              {searchRows.map((hit) => {
                const isDir = !!hit.isDir
                const Glyph = isDir ? Folder : fileIconForPath(hit.path)
                const statusClass = gitStatusClass(hit.path, isDir, gitIndex)
                return (
                  <li key={hit.path}>
                    <button
                      type="button"
                      onClick={() => {
                        if (isDir) {
                          setQuery("")
                          setExpanded((prev) => new Set(prev).add(hit.path))
                        } else {
                          onOpenFile(hit.path)
                        }
                      }}
                      onContextMenu={(e) => handleContextMenu(e, hit)}
                      title={hit.path}
                      className={cn(
                        "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-sm",
                        "hover:bg-fill-4",
                        statusClass ?? "text-ink-secondary hover:text-ink",
                      )}
                    >
                      <Glyph
                        className={cn(
                          "h-3.5 w-3.5 shrink-0",
                          statusClass ?? "text-ink-faint",
                        )}
                        aria-hidden
                      />
                      <span className="min-w-0 flex-1 truncate">
                        <span className="text-ink-faint">
                          {hit.path.includes("/")
                            ? hit.path.slice(0, hit.path.lastIndexOf("/") + 1)
                            : ""}
                        </span>
                        {hit.name}
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          )
        ) : resolvingCwd ? (
          <div className="flex items-center gap-2 px-4 py-8 text-xs text-ink-muted">
            <Spinner size="sm" />
            Opening folder…
          </div>
        ) : !resolvedCwd ? (
          <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
            <Folder className="h-6 w-6 text-ink-faint" aria-hidden />
            <p className="text-sm text-ink-secondary">Workspace folder missing</p>
            <p className="max-w-sm text-xs text-ink-muted">
              Could not open{" "}
              <span className="break-all text-ink-secondary">{cwd}</span>
              {fallbackCwd ? (
                <>
                  {" "}
                  (or fallback{" "}
                  <span className="break-all text-ink-secondary">
                    {fallbackCwd}
                  </span>
                  )
                </>
              ) : null}
              . Re-open the project folder for this agent.
            </p>
          </div>
        ) : (
          <TreeBranch
            cwd={cwd}
            fallbackCwd={fallbackCwd}
            dirPath=""
            depth={0}
            expanded={expanded}
            gitIndex={gitIndex}
            onToggle={toggleDir}
            onOpenFile={onOpenFile}
            onContextMenu={handleContextMenu}
          />
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
