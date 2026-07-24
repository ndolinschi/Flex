import {
  useMemo,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import { ChevronDown, Folder, FolderOpen } from "lucide-react"
import { listDirChildren } from "../../../lib/tauri"
import { sortFileHits } from "../../../lib/fileTree"
import type { FileHit } from "../../../lib/types"
import { cn, fileIconForPath } from "../../../lib/utils"
import { Spinner } from "../../atoms"
import { EmptyState, ToolQueryError } from "../../molecules"
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

  const {
    data: children = [],
    isLoading,
    isFetching,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: ["workspace-dir-children", cwd, fallbackCwd ?? "", dirPath],
    queryFn: () => listDirChildren(cwd, dirPath, fallbackCwd),
    enabled: !!cwd && shouldLoad,
    staleTime: 60_000,
  })

  const sorted = useMemo(
    () => sortFileHits(children.filter((h) => h.name !== "$null")),
    [children],
  )

  if (!shouldLoad) return null

  if (isError && sorted.length === 0) {
    if (isRoot) {
      return (
        <ToolQueryError
          title="Couldn't list files"
          error={error}
          fallbackMessage="Failed to list workspace files."
          onRetry={() => void refetch()}
          retrying={isFetching}
          className="py-12"
        />
      )
    }
    return (
      <div
        className="px-2 py-1 text-xs text-danger/90"
        style={{ paddingLeft: 8 + (depth + 1) * INDENT_PX }}
      >
        Failed to load folder
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-1 h-5 px-1 text-xs"
          onClick={() => void refetch()}
        >
          Retry
        </Button>
      </div>
    )
  }

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

  const handleTreeKeyDown = (e: ReactKeyboardEvent<HTMLUListElement>) => {
    if (!isRoot) return
    const target = (e.target as HTMLElement).closest<HTMLElement>(
      '[role="treeitem"]',
    )
    if (!target) return
    const items = Array.from(
      e.currentTarget.querySelectorAll<HTMLElement>('[role="treeitem"]'),
    )
    const index = items.indexOf(target)
    if (index < 0) return

    const focusAt = (next: number) => {
      const item = items[next]
      if (!item) return
      e.preventDefault()
      item.focus()
    }

    if (e.key === "ArrowDown") focusAt(Math.min(index + 1, items.length - 1))
    else if (e.key === "ArrowUp") focusAt(Math.max(index - 1, 0))
    else if (e.key === "Home") focusAt(0)
    else if (e.key === "End") focusAt(items.length - 1)
    else if (e.key === "ArrowRight") {
      const expandedState = target.getAttribute("aria-expanded")
      if (expandedState === "false") {
        e.preventDefault()
        target.click()
      } else if (expandedState === "true") {
        const level = Number(target.getAttribute("aria-level") ?? 1)
        const next = items[index + 1]
        if (Number(next?.getAttribute("aria-level") ?? 0) > level) {
          focusAt(index + 1)
        }
      }
    } else if (e.key === "ArrowLeft") {
      if (target.getAttribute("aria-expanded") === "true") {
        e.preventDefault()
        target.click()
        return
      }
      const level = Number(target.getAttribute("aria-level") ?? 1)
      for (let i = index - 1; i >= 0; i -= 1) {
        if (Number(items[i]?.getAttribute("aria-level") ?? 0) === level - 1) {
          focusAt(i)
          break
        }
      }
    }
  }

  return (
    <ul
      className="flex flex-col"
      role={isRoot ? "tree" : "group"}
      aria-label={isRoot ? "Workspace files" : undefined}
      onKeyDown={isRoot ? handleTreeKeyDown : undefined}
    >
      {sorted.map((hit, index) => {
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
              role="treeitem"
              tabIndex={
                isActive || (isRoot && index === 0 && !activePath) ? 0 : -1
              }
              onClick={() => {
                if (isDir) onToggle(hit.path)
                else onOpenFile(hit.path)
              }}
              onContextMenu={(e) => onContextMenu(e, hit)}
              title={hit.path}
              aria-expanded={isDir ? isOpen : undefined}
              aria-level={depth + 1}
              aria-selected={!isDir ? isActive : undefined}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                "h-7 w-full justify-start gap-1.5 rounded-sm pr-2 text-sm font-normal leading-[1.5]",
                "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                isActive
                  ? "border-l-2 border-l-accent bg-fill-2 text-ink hover:bg-fill-2"
                  : "border-l-2 border-l-transparent hover:bg-fill-4",
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
