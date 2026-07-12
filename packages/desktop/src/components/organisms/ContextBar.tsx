import { useQuery } from "@tanstack/react-query"
import { gitIsRepo, isIsolated } from "../../lib/tauri"
import { BranchPicker } from "../molecules/BranchPicker"
import { ProjectPicker } from "../molecules/ProjectPicker"
import { UsageRing } from "./context-bar/UsageRing"
import { IsolationBadge } from "./context-bar/IsolationBadge"
import { IsolationPicker } from "./context-bar/IsolationPicker"
import { CommitBar } from "./context-bar/CommitBar"

type ContextBarProps = {
  /** Working directory for tools/git (worktree root when isolated). */
  cwd?: string
  /** Project root for the picker label (`base_cwd ?? cwd`). */
  projectCwd?: string
  sessionId?: string | null
  disabled?: boolean
  onError?: (message: string) => void
}

/** Context row above the composer: project · branch · isolation · context %. */
export const ContextBar = ({
  cwd,
  projectCwd,
  sessionId,
  disabled = false,
  onError,
}: ContextBarProps) => {
  const { data: isolated } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })

  // Gate the entire git cluster (branch pill + commit bar) on the cwd
  // actually being a git repo — a non-git folder should show none of it
  // rather than a misleading "No branch" pill. `isRepo` defaults to `true`
  // while the query is loading (or has no cwd yet) so the chrome doesn't
  // flash away/in on every session switch; it only ever hides once we
  // positively know there's no repo.
  const { data: isRepo = true } = useQuery({
    queryKey: ["git-is-repo", cwd],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  })

  return (
    <div className="flex min-h-[var(--status-bar-height)] items-center gap-2 px-1">
      {/* min-w-0 + flex-1 (not justify-between) so this group is what shrinks
          under pressure — the gap to the right-hand cluster is a real flex
          gap, not `justify-between`'s leftover space, so it can never
          collapse to 0 and let the two clusters visually collide. */}
      <div className="flex min-w-0 flex-1 items-center gap-0.5">
        <ProjectPicker
          sessionId={sessionId ?? null}
          cwd={projectCwd || cwd}
          disabled={disabled}
          onError={onError}
        />
        {isRepo ? (
          <BranchPicker cwd={cwd} disabled={disabled} onError={onError} />
        ) : null}
        {isolated && sessionId ? (
          <IsolationBadge sessionId={sessionId} onError={onError} />
        ) : null}
        {!isolated && sessionId ? (
          <IsolationPicker sessionId={sessionId} disabled={disabled} />
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-2">
        {isRepo && !isolated && sessionId ? (
          <CommitBar sessionId={sessionId} cwd={cwd} onError={onError} />
        ) : null}
        <UsageRing sessionId={sessionId} />
      </div>
    </div>
  )
}
