import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { FileCode2, Folder, Search } from "lucide-react"
import { listFiles } from "../../../lib/tauri"
import { basename, cn, fileIconForPath } from "../../../lib/utils"
import { Spinner } from "../../atoms"

type FileExplorerProps = {
  cwd: string
  /** Called with a repo-relative file path (never a directory). */
  onOpenFile: (path: string) => void
}

/** Lightweight workspace file browser for the Files right-panel tab.
 * Uses the same `list_files` IPC as composer @-mentions. */
export const FileExplorer = ({ cwd, onOpenFile }: FileExplorerProps) => {
  const [query, setQuery] = useState("")
  const trimmed = query.trim()

  const { data: hits = [], isLoading, isFetching } = useQuery({
    queryKey: ["workspace-file-list", cwd, trimmed],
    queryFn: () => listFiles(cwd, trimmed),
    enabled: !!cwd,
    staleTime: 10_000,
  })

  const files = useMemo(
    () => hits.filter((h) => !h.is_dir && !h.path.endsWith("/")),
    [hits],
  )
  const dirs = useMemo(
    () => hits.filter((h) => h.is_dir || h.path.endsWith("/")),
    [hits],
  )

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2 py-1.5">
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
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-1">
        {isLoading && files.length === 0 && dirs.length === 0 ? (
          <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
            <Spinner size="sm" />
            Loading…
          </div>
        ) : files.length === 0 && dirs.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
            <FileCode2 className="h-6 w-6 text-ink-faint" aria-hidden />
            <p className="text-sm text-ink-secondary">
              {trimmed ? "No matches" : "No files found"}
            </p>
            <p className="text-xs text-ink-muted">
              {trimmed
                ? "Try a different search."
                : "Open a project folder in this session to browse files."}
            </p>
          </div>
        ) : (
          <ul className="flex flex-col" role="list">
            {dirs.slice(0, 20).map((hit) => (
              <li key={hit.path}>
                <div
                  className="flex h-7 items-center gap-2 rounded-md px-2 text-sm text-ink-muted"
                  title={hit.path}
                >
                  <Folder className="h-3.5 w-3.5 shrink-0" aria-hidden />
                  <span className="min-w-0 truncate">{hit.path}</span>
                </div>
              </li>
            ))}
            {files.map((hit) => {
              const Glyph = fileIconForPath(hit.path)
              return (
                <li key={hit.path}>
                  <button
                    type="button"
                    onClick={() => onOpenFile(hit.path)}
                    title={hit.path}
                    className={cn(
                      "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-sm",
                      "text-ink-secondary hover:bg-fill-4 hover:text-ink",
                    )}
                  >
                    <Glyph className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
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
    </div>
  )
}
