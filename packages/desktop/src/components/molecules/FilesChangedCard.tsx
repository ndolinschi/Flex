import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { ArrowRight, ChevronDown, ChevronRight } from "lucide-react"
import { gitStatusSinceBaseline } from "../../lib/tauri"
import { useIsGitRepo } from "../../hooks/useIsGitRepo"
import { sessionScopeKey, useAppStore } from "../../stores/appStore"
import { basename, cn, fileIconForPath } from "../../lib/utils"
import { DiffStat } from "../atoms"
import { Button } from "@/components/ui/button"

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
 * headline to expand an inline file list; click a file to open it in the
 * Files (Monaco) tab. "Review" still opens Changes for undo/diff/commit.
 */
export const FilesChangedCard = ({ cwd, sessionId }: FilesChangedCardProps) => {
  const [expanded, setExpanded] = useState(false)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const openWorkspaceFile = useAppStore((s) => s.openWorkspaceFile)

  const { data: isRepo } = useIsGitRepo(cwd)

  const { data: summary } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo !== false,
    staleTime: 30_000,
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
    if (!sessionId) return
    openToolBesideChat(sessionId, "changes")
  }

  const handleOpenFile = (path: string, status: string, isDir: boolean) => {
    if (!sessionId || isDir || status === "D") return
    openWorkspaceFile(sessionScopeKey(sessionId), path)
  }

  const hiddenCount = Math.max(0, totalCount - files.length)

  return (
    <div className="overflow-hidden rounded-[var(--radius-lg)] border border-stroke-3 bg-transparent">
      {/* Shared horizontal recipe with file rows: px-2 shell + px-1.5 row
          gutters so chevron/icon and Review/status share one vertical axis. */}
      <div className="flex min-h-[var(--end-of-turn-reserved-height)] items-center gap-2 px-2">
        <Button
          variant="ghost"
          onClick={handleToggle}
          aria-expanded={expanded}
          aria-label={
            expanded
              ? "Collapse changed files"
              : `Expand ${totalCount} changed file${totalCount === 1 ? "" : "s"}`
          }
          className="h-auto min-w-0 flex-1 justify-start gap-1.5 rounded-md px-1.5 py-1 font-normal hover:bg-fill-4"
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
          )}
          <span className="truncate text-base text-ink">
            {totalCount} file{totalCount === 1 ? "" : "s"} changed
          </span>
          <DiffStat summary={totals} size="sm" className="shrink-0" />
        </Button>
        <Button
          variant="ghost"
          onClick={handleReview}
          className="h-auto shrink-0 gap-1 rounded-md px-1.5 py-1 text-xs text-accent font-normal hover:bg-transparent hover:opacity-80"
        >
          Review
          <ArrowRight className="h-3 w-3" aria-hidden />
        </Button>
      </div>

      {expanded ? (
        <ul className="border-t border-stroke-3 px-2 py-1" role="list">
          {files.map((file) => {
            const isDir = file.path.endsWith("/")
            const name = isDir ? file.path : basename(file.path)
            const dir =
              !isDir && file.path.includes("/")
                ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
                : ""
            const FileGlyph = fileIconForPath(file.path)
            const statusClass = STATUS_COLOR[file.status] ?? "text-ink-muted"
            const canOpen = !isDir && file.status !== "D" && !!sessionId
            const hasLineStats =
              (file.added ?? 0) > 0 || (file.removed ?? 0) > 0
            return (
              <li key={file.path} className="list-none">
                <Button
                  variant="ghost"
                  disabled={!canOpen}
                  title={canOpen ? `Open ${file.path}` : file.path}
                  onClick={() =>
                    handleOpenFile(file.path, file.status, isDir)
                  }
                  className={cn(
                    "h-7 w-full justify-start gap-1.5 rounded-md px-1.5 text-base font-normal",
                    canOpen
                      ? "hover:bg-fill-4"
                      : "cursor-default opacity-70 hover:bg-transparent",
                  )}
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
                  {hasLineStats ? (
                    <DiffStat
                      summary={{
                        added: file.added ?? 0,
                        removed: file.removed ?? 0,
                      }}
                      size="xs"
                      className="shrink-0"
                    />
                  ) : (
                    <span
                      className={cn(
                        "w-3.5 shrink-0 text-center font-mono text-xs",
                        statusClass,
                      )}
                    >
                      {file.status === "?" ? "U" : file.status}
                    </span>
                  )}
                </Button>
              </li>
            )
          })}
          {truncated || hiddenCount > 0 ? (
            <li className="px-1.5 py-1 text-xs text-ink-muted">
              <Button
                variant="link"
                onClick={handleReview}
                className="h-auto px-0 py-0 text-xs text-accent font-normal"
              >
                +{hiddenCount > 0 ? hiddenCount : "more"} in Changes
              </Button>
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  )
}
