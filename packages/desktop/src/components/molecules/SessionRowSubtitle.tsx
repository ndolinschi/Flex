import type { GitStatusSummary, WorkspaceStatusDto } from "../../lib/types"
import { formatCompactTime } from "../../lib/utils"
import { DiffStat } from "../atoms"

/** Parsed `+N -M` diff counters, if the workspace summary is in that shape. */
export const parseDiffStat = (
  summary: string,
): { added: number; removed: number } | null => {
  const match = summary.match(/\+(\d+)\D+-(\d+)/)
  if (!match) return null
  return { added: Number(match[1]), removed: Number(match[2]) }
}

type SessionRowSubtitleProps = {
  updatedAtMs: number
  workspaceStatus?: WorkspaceStatusDto | null
  gitStatus?: GitStatusSummary
}

/** Diff + relative-time line under a session row title. */
export const SessionRowSubtitle = ({
  updatedAtMs,
  workspaceStatus,
  gitStatus,
}: SessionRowSubtitleProps) => {
  const diffStat = workspaceStatus ? parseDiffStat(workspaceStatus.summary) : null
  // Isolated sessions show their private-worktree diff (`workspaceStatus`);
  // everything else falls back to the same session-scoped git-status summary
  // the Changes tab reads.
  const gitDiffStat =
    !workspaceStatus && gitStatus && gitStatus.totalCount > 0
      ? {
          added: gitStatus.totalAdded,
          removed: gitStatus.totalRemoved,
          filesChanged: gitStatus.totalCount,
        }
      : null

  return (
    <span className="flex min-w-0 items-center gap-1 truncate pl-[26px] text-xs text-ink-muted">
      <DiffStat
        summary={
          diffStat
            ? diffStat
            : gitDiffStat
              ? gitDiffStat
              : {
                  added: 0,
                  removed: 0,
                  filesChanged: workspaceStatus?.filesChanged ?? 0,
                }
        }
      />
      <span>
        {" · "}
        {formatCompactTime(updatedAtMs)}
      </span>
    </span>
  )
}

/** Whether a session row should show the diff subtitle (vs trailing time). */
export const sessionRowHasSubtitle = (
  workspaceStatus?: WorkspaceStatusDto | null,
  gitStatus?: GitStatusSummary,
): boolean => {
  if (workspaceStatus) return true
  return !!gitStatus && gitStatus.totalCount > 0
}
