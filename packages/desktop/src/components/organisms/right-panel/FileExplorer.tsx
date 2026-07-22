import { useEffect, useMemo, useState, type MouseEvent } from "react"
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronDown, ChevronRight, FilePlus, Folder, Pencil, Search, Trash2 } from "lucide-react"
import {
  createTextFile,
  deletePath,
  gitStatusSinceBaseline,
  invalidateWorkspacePathCache,
  listFiles,
  renamePath,
  resolveWorkspaceCwd,
  toInvokeError,
} from "../../../lib/tauri"
import { sortFileHits } from "../../../lib/fileTree"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import type { FileHit } from "../../../lib/types"
import { basename, cn } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { Spinner } from "../../atoms"
import { ContextMenu, EmptyState, type ContextMenuItem } from "../../molecules"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  FileExplorerDialogs,
  type FileExplorerDialogState,
} from "./FileExplorerDialogs"
import { FileExplorerSearchResults } from "./FileExplorerSearchResults"
import {
  buildGitStatusIndex,
  dirPrefix,
  isValidBasename,
  isValidRelativeFilePath,
} from "./fileExplorerGit"
import { TreeBranch } from "./TreeBranch"

type FileExplorerProps = {
  sessionId: string
  sessionKey: string
  cwd: string
  /** Prefer when `cwd` is a missing isolated worktree (`session.base_cwd`). */
  fallbackCwd?: string
  /** Called with a repo-relative file path (never a directory). */
  onOpenFile: (path: string) => void
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
  const [dialog, setDialog] = useState<FileExplorerDialogState>(null)
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
    staleTime: 60_000,
    placeholderData: keepPreviousData,
  })

  // Shared with Changes / sidebar — color dirty files in the tree.
  const { data: gitSummary } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId),
    enabled: !!cwd && !!sessionId,
    staleTime: 60_000,
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
            icon: expanded.has(menuHit.path) ? ChevronDown : ChevronRight,
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
        <InputGroup
          className={cn(
            "h-6 min-w-0 flex-1 border-0 bg-transparent shadow-none dark:bg-transparent",
            "has-[[data-slot=input-group-control]:focus-visible]:border-transparent",
            "has-[[data-slot=input-group-control]:focus-visible]:ring-0",
          )}
        >
          <InputGroupAddon align="inline-start" className="pl-0 py-0">
            <Search className="size-3.5 text-ink-faint" aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search files…"
            aria-label="Search workspace files"
            className="h-6 px-0 text-sm text-ink placeholder:text-ink-faint"
          />
          {searchFetching ? (
            <InputGroupAddon align="inline-end" className="pr-0 py-0">
              <Spinner size="sm" />
            </InputGroupAddon>
          ) : null}
          <InputGroupAddon align="inline-end" className="pr-0 py-0">
            <InputGroupButton
              size="icon-xs"
              aria-label="New file"
              title="New file"
              onClick={() => openCreate()}
              className="text-ink-muted hover:bg-fill-4 hover:text-ink"
            >
              <FilePlus aria-hidden />
            </InputGroupButton>
          </InputGroupAddon>
        </InputGroup>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="px-2.5 py-1">
        {searching ? (
          <FileExplorerSearchResults
            loading={searchLoading}
            rows={searchRows}
            gitIndex={gitIndex}
            onOpenFile={onOpenFile}
            onOpenDir={(path) => {
              setQuery("")
              setExpanded((prev) => new Set(prev).add(path))
            }}
            onContextMenu={handleContextMenu}
          />
        ) : resolvingCwd ? (
          <div className="flex items-center gap-2 px-4 py-8 text-xs text-ink-muted">
            <Spinner size="sm" />
            Opening folder…
          </div>
        ) : !resolvedCwd ? (
          <EmptyState
            icon={<Folder className="h-6 w-6" aria-hidden />}
            title="Workspace folder missing"
            description={
              fallbackCwd
                ? `Could not open ${cwd} (or fallback ${fallbackCwd}). Re-open the project folder for this agent.`
                : `Could not open ${cwd}. Re-open the project folder for this agent.`
            }
          />
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
      </ScrollArea>

      <ContextMenu
        position={menu ? { x: menu.x, y: menu.y } : null}
        items={contextMenuItems}
        onClose={() => setMenu(null)}
      />

      <FileExplorerDialogs
        dialog={dialog}
        draftPath={draftPath}
        setDraftPath={setDraftPath}
        busy={busy}
        confirmDisabled={confirmDisabled}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (!busy) setDialog(null)
        }}
      />
    </div>
  )
}
