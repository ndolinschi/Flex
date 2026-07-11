import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { gitIsRepo, gitStatusSinceBaseline } from "../../lib/tauri"
import type { GitFileStatus } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { basename, cn, fileIconForPath } from "../../lib/utils"

/** Mirrors `STATUS_COLOR`/status-letter mapping in RightPanel's `ChangesTab`
 * (kept as a small local copy — importing from an organism into a molecule
 * would invert the dependency direction). */
const STATUS_COLOR: Record<string, string> = {
  M: "text-yellow",
  A: "text-green",
  "?": "text-green",
  D: "text-red",
  R: "text-blue",
}

const MAX_VISIBLE = 5

const FileRow = ({ file }: { file: GitFileStatus }) => {
  // A trailing-slash path is an untracked-directory porcelain entry (e.g.
  // "public/"), not a file — render it as just "public/" instead of
  // splitting it into a "public/" prefix + "public" basename, which would
  // duplicate the name (see `capture_session_baseline`'s "dir" sentinel).
  const isDir = file.path.endsWith("/")
  const dir = !isDir && file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
    : ""
  const name = isDir ? file.path : basename(file.path)
  const FileGlyph = fileIconForPath(file.path)

  return (
    <li className="flex h-5 items-center gap-1.5 text-xs">
      <FileGlyph className="h-3 w-3 shrink-0 text-ink-muted" aria-hidden />
      <span className="flex min-w-0 flex-1 items-center gap-0">
        {dir ? (
          <span
            className="min-w-0 shrink overflow-hidden text-ellipsis whitespace-nowrap opacity-40"
            style={{ direction: "rtl" }}
          >
            <span style={{ direction: "ltr", unicodeBidi: "embed" }}>{dir}</span>
          </span>
        ) : null}
        <span className="shrink-0 truncate text-ink-secondary">{name}</span>
      </span>
      <span
        className={cn(
          "w-3 shrink-0 text-center font-mono text-[11px]",
          STATUS_COLOR[file.status] ?? "text-ink-muted",
        )}
      >
        {file.status === "?" ? "U" : file.status}
      </span>
    </li>
  )
}

type FilesChangedCardProps = {
  cwd?: string
  sessionId?: string | null
}

/**
 * "N Files Changed" card, rendered in the chat feed after the
 * latest turn once the session stops streaming and the working tree has
 * pending changes. Shares the `["git-status", cwd, sessionId]` query with
 * the right panel's Changes tab (same key → same cache entry, no extra
 * polling).
 */
export const FilesChangedCard = ({ cwd, sessionId }: FilesChangedCardProps) => {
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)
  const [expanded, setExpanded] = useState(false)

  // Gate on the cwd being a git repo — see ContextBar/RightPanel's `isRepo`
  // gating for the full rationale; this card must return null (not an
  // error) for a non-git session's cwd.
  const { data: isRepo } = useQuery({
    queryKey: ["git-is-repo", cwd ?? ""],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 15_000,
  })

  const { data: files = [] } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo !== false,
    staleTime: 30_000,
  })

  if (!isRepo || files.length === 0) return null

  const visible = expanded ? files : files.slice(0, MAX_VISIBLE)
  const remaining = files.length - MAX_VISIBLE

  const handleReview = () => {
    setRightPanelOpen(true)
    setRightPanelTab("changes")
  }

  return (
    <div className="rounded-lg border border-stroke-3 bg-transparent px-3 py-2">
      <div className="flex h-5 items-center justify-between">
        <span className="text-[13px] text-ink">
          {files.length} File{files.length === 1 ? "" : "s"} Changed
        </span>
        <button
          type="button"
          onClick={handleReview}
          className="text-xs text-accent transition-opacity hover:opacity-80"
        >
          Review
        </button>
      </div>
      <ul className="mt-1.5 flex flex-col gap-1">
        {visible.map((file) => (
          <FileRow key={file.path} file={file} />
        ))}
      </ul>
      {!expanded && remaining > 0 ? (
        <button
          type="button"
          onClick={() => setExpanded(true)}
          className="mt-1.5 text-xs text-ink-faint transition-colors hover:text-ink-secondary"
        >
          Show {remaining} more
        </button>
      ) : null}
    </div>
  )
}
