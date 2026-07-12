import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { ArrowRight, ChevronDown, ChevronRight } from "lucide-react"
import { gitIsRepo, gitStatusSinceBaseline } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { basename, cn, fileIconForPath } from "../../lib/utils"
import { DiffStat } from "../atoms"

type FilesChangedCardProps = {
  cwd?: string
  sessionId?: string | null
}

const STATUS_COLOR: Record<string, string> = {
  M: "text-yellow",
  A: "text-green",
  "?": "text-green",
  D: "text-red",
  R: "text-blue",
}

/**
 * Compact "N files changed" summary in the chat feed after a turn. Click the
 * headline to expand an inline file list; "Review" still opens the Changes
 * tab for full undo/diff/commit actions.
 */
export const FilesChangedCard = ({ cwd, sessionId }: FilesChangedCardProps) => {
  const [expanded, setExpanded] = useState(false)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)

  const { data: isRepo } = useQuery({
    queryKey: ["git-is-repo", cwd ?? ""],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  })

  const { data: summary } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo !== false,
    staleTime: 30_000,
    refetchOnMount: "always",
  })

  const totalCount = summary?.totalCount ?? 0
  const files = summary?.files ?? []
  const truncated = summary?.truncated ?? false

  if (!isRepo || totalCount === 0) return null

  const totals = {
    added: summary?.totalAdded ?? 0,
    removed: summary?.totalRemoved ?? 0,
  }

  const handleToggle = () => {
    setExpanded((v) => !v)
  }

  const handleReview = () => {
    setRightPanelOpen(true)
    setRightPanelTab("changes")
  }

  const hiddenCount = Math.max(0, totalCount - files.length)

  return (
    <div className="overflow-hidden rounded-lg border border-stroke-3 bg-transparent">
      <div className="flex h-9 items-center gap-1 px-1.5">
        <button
          type="button"
          onClick={handleToggle}
          aria-expanded={expanded}
          aria-label={
            expanded
              ? "Collapse changed files"
              : `Expand ${totalCount} changed file${totalCount === 1 ? "" : "s"}`
          }
          className={cn(
            "flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1.5 py-1",
            "text-left transition-colors hover:bg-fill-3",
          )}
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
          )}
          <span className="truncate text-[13px] text-ink">
            {totalCount} file{totalCount === 1 ? "" : "s"} changed
          </span>
          <DiffStat summary={totals} size="sm" />
        </button>
        <button
          type="button"
          onClick={handleReview}
          className="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs text-accent transition-opacity hover:opacity-80"
        >
          Review
          <ArrowRight className="h-3 w-3" aria-hidden />
        </button>
      </div>

      {expanded ? (
        <ul className="border-t border-stroke-3 px-2 py-1.5" role="list">
          {files.map((file) => {
            const isDir = file.path.endsWith("/")
            const name = isDir ? file.path : basename(file.path)
            const dir =
              !isDir && file.path.includes("/")
                ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
                : ""
            const FileGlyph = fileIconForPath(file.path)
            const statusClass = STATUS_COLOR[file.status] ?? "text-ink-muted"
            return (
              <li
                key={file.path}
                className="flex h-7 items-center gap-2 rounded-md px-1.5 text-[13px]"
              >
                <FileGlyph
                  className={cn("h-3.5 w-3.5 shrink-0", statusClass)}
                  aria-hidden
                />
                <span className="min-w-0 flex-1 truncate">
                  {dir ? (
                    <span className="text-ink-faint">{dir}</span>
                  ) : null}
                  <span className="text-ink">{name}</span>
                </span>
                {(file.added ?? 0) > 0 || (file.removed ?? 0) > 0 ? (
                  <DiffStat
                    summary={{
                      added: file.added ?? 0,
                      removed: file.removed ?? 0,
                    }}
                    size="xs"
                  />
                ) : (
                  <span
                    className={cn(
                      "shrink-0 font-mono text-[11px]",
                      statusClass,
                    )}
                  >
                    {file.status}
                  </span>
                )}
              </li>
            )
          })}
          {truncated || hiddenCount > 0 ? (
            <li className="px-1.5 py-1 text-xs text-ink-muted">
              <button
                type="button"
                onClick={handleReview}
                className="text-accent transition-opacity hover:opacity-80"
              >
                +{hiddenCount > 0 ? hiddenCount : "more"} in Changes
              </button>
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  )
}
