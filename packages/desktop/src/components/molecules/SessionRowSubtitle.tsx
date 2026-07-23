import type {
  GitStatusSummary,
  SessionMeta,
  WorkspaceStatusDto,
} from "../../lib/types"
import { isPristineSession } from "../../lib/types"
import { formatCompactTime } from "../../lib/utils"
import { DiffStat } from "../atoms"

export const parseDiffStat = (
  summary: string,
): { added: number; removed: number } | null => {
  const match = summary.match(/\+(\d+)\D+-(\d+)/)
  if (!match) return null
  return { added: Number(match[1]), removed: Number(match[2]) }
}

export const sessionTrailingDiff = (
  session: Pick<SessionMeta, "title" | "base_cwd" | "workspace_id">,
  workspaceStatus?: WorkspaceStatusDto | null,
  gitStatus?: GitStatusSummary,
): { added: number; removed: number; filesChanged?: number } | null => {
  if (isPristineSession(session)) return null

  const workspaceDiff = workspaceStatus
    ? parseDiffStat(workspaceStatus.summary)
    : null
  if (workspaceDiff && (workspaceDiff.added > 0 || workspaceDiff.removed > 0)) {
    return workspaceDiff
  }
  if (
    workspaceStatus &&
    !workspaceDiff &&
    (workspaceStatus.filesChanged ?? 0) > 0
  ) {
    return {
      added: 0,
      removed: 0,
      filesChanged: workspaceStatus.filesChanged,
    }
  }
  if (!workspaceStatus && gitStatus && gitStatus.totalCount > 0) {
    const added = gitStatus.totalAdded
    const removed = gitStatus.totalRemoved
    if (added > 0 || removed > 0) {
      return { added, removed, filesChanged: gitStatus.totalCount }
    }
  }
  return null
}

type SessionRowSubtitleProps = {
  session: Pick<SessionMeta, "title" | "base_cwd" | "workspace_id">
  updatedAtMs: number
  workspaceStatus?: WorkspaceStatusDto | null
  gitStatus?: GitStatusSummary
  repoLabel?: string
}

export const SessionRowSubtitle = ({
  session,
  updatedAtMs,
  workspaceStatus,
  gitStatus,
  repoLabel,
}: SessionRowSubtitleProps) => {
  const trailing = sessionTrailingDiff(session, workspaceStatus, gitStatus)
  const hasDiff = !!trailing

  return (
    <span className="flex min-w-0 items-center gap-1 truncate pl-[26px] text-xs text-ink-muted">
      {repoLabel ? (
        <span className="truncate" title={repoLabel}>
          {repoLabel}
        </span>
      ) : null}
      {hasDiff && trailing ? <DiffStat summary={trailing} /> : null}
      <span>
        {repoLabel || hasDiff ? " · " : null}
        {formatCompactTime(updatedAtMs)}
      </span>
    </span>
  )
}

export const sessionRowHasSubtitle = (
  session: Pick<SessionMeta, "title" | "base_cwd" | "workspace_id">,
  workspaceStatus?: WorkspaceStatusDto | null,
  gitStatus?: GitStatusSummary,
  repoLabel?: string,
): boolean => {
  if (repoLabel) return true
  return !!sessionTrailingDiff(session, workspaceStatus, gitStatus)
}
