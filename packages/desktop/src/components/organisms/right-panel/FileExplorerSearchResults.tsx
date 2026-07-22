import type { MouseEvent } from "react"
import { FileCode2, Folder } from "lucide-react"
import type { FileHit } from "../../../lib/types"
import { cn, fileIconForPath } from "../../../lib/utils"
import { Spinner } from "../../atoms"
import { EmptyState } from "../../molecules"
import { Button } from "@/components/ui/button"
import { gitStatusClass, type GitStatusIndex } from "./fileExplorerGit"

type FileExplorerSearchResultsProps = {
  loading: boolean
  rows: FileHit[]
  gitIndex: GitStatusIndex
  onOpenFile: (path: string) => void
  onOpenDir: (path: string) => void
  onContextMenu: (e: MouseEvent, hit: FileHit) => void
}

export const FileExplorerSearchResults = ({
  loading,
  rows,
  gitIndex,
  onOpenFile,
  onOpenDir,
  onContextMenu,
}: FileExplorerSearchResultsProps) => {
  if (loading && rows.length === 0) {
    return (
      <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
        <Spinner size="sm" />
        Searching…
      </div>
    )
  }

  if (rows.length === 0) {
    return (
      <EmptyState
        className="py-12"
        icon={<FileCode2 className="h-6 w-6" aria-hidden />}
        title="No matches"
        description="Try a different search."
      />
    )
  }

  return (
    <ul className="flex flex-col" role="list">
      {rows.map((hit) => {
        const isDir = !!hit.isDir
        const Glyph = isDir ? Folder : fileIconForPath(hit.path)
        const statusClass = gitStatusClass(hit.path, isDir, gitIndex)
        return (
          <li key={hit.path}>
            <Button
              variant="ghost"
              onClick={() => {
                if (isDir) onOpenDir(hit.path)
                else onOpenFile(hit.path)
              }}
              onContextMenu={(e) => onContextMenu(e, hit)}
              title={hit.path}
              className={cn(
                "h-7 w-full justify-start gap-2 rounded-md px-2 text-sm font-normal",
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
            </Button>
          </li>
        )
      })}
    </ul>
  )
}
