import { useMemo, type MouseEvent } from "react"
import { useQuery } from "@tanstack/react-query"
import { ChevronDown, Folder, FolderOpen } from "lucide-react"
import { listDirChildren } from "../../../lib/tauri"
import { sortFileHits } from "../../../lib/fileTree"
import type { FileHit } from "../../../lib/types"
import { cn, fileIconForPath } from "../../../lib/utils"
import { Spinner } from "../../atoms"
import { EmptyState } from "../../molecules"
import { Button } from "@/components/ui/button"
import { gitStatusClass, type GitStatusIndex } from "./fileExplorerGit"

const INDENT_PX = 12

export type TreeBranchProps = {
  cwd: string
  fallbackCwd?: string
  dirPath: string
  depth: number
  expanded: Set<string>
  gitIndex: GitStatusIndex
  activePath?: string
  onToggle: (dirPath: string) => void
  onOpenFile: (path: string) => void
  onContextMenu: (e: MouseEvent, hit: FileHit) => void
}

/** One directory level — loads children on demand when expanded (or root). */
export const TreeBranch = ({
  cwd,
  fallbackCwd,
  dirPath,
  depth,
  expanded,
  gitIndex,
  activePath,
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
    staleTime: 60_000,
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
        <EmptyState
          className="py-12"
          icon={<Folder className="h-6 w-6" aria-hidden />}
          title="This folder is empty"
          description="Create a file to get started."
        />
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
        const isActive = !isDir && activePath === hit.path
        const Glyph = isDir
          ? isOpen
            ? FolderOpen
            : Folder
          : fileIconForPath(hit.path)
        const statusClass = gitStatusClass(hit.path, isDir, gitIndex)
        return (
          <li key={hit.path}>
            <Button
              variant="ghost"
              onClick={() => {
                if (isDir) onToggle(hit.path)
                else onOpenFile(hit.path)
              }}
              onContextMenu={(e) => onContextMenu(e, hit)}
              title={hit.path}
              aria-expanded={isDir ? isOpen : undefined}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                // File-tree cell: h-7, r6, whisper fills — open dirs read selected (fill-2).
                "h-7 w-full justify-start gap-1.5 rounded-sm pr-2 text-sm font-normal leading-[1.5]",
                "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                isOpen || isActive
                  ? "bg-fill-2 text-ink hover:bg-fill-2"
                  : "hover:bg-fill-4",
                statusClass ??
                  (!(isOpen || isActive) && "text-ink-secondary hover:text-ink"),
              )}
              style={{ paddingLeft: 8 + depth * INDENT_PX }}
            >
              <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
                {isDir ? (
                  <ChevronDown
                    className={cn(
                      "h-3 w-3 text-icon-3 opacity-70 transition-transform duration-[var(--duration-fast)] ease-[var(--easing-default)]",
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
            </Button>
            {isDir && isOpen ? (
              <TreeBranch
                cwd={cwd}
                fallbackCwd={fallbackCwd}
                dirPath={hit.path}
                depth={depth + 1}
                expanded={expanded}
                gitIndex={gitIndex}
                activePath={activePath}
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
