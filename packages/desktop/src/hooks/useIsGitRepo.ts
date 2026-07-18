import { useQuery } from "@tanstack/react-query"
import { gitIsRepo } from "../lib/tauri"

/** Shared `git-is-repo` observer — ContextBar, FilesChangedCard, and ChangesTab
 * all need the same gate. Identical queryKey + options so TanStack Query
 * dedupes the refetch to one interval per cwd.
 *
 * Repo membership rarely flips; prefer a long staleTime over a hot 5s poll
 * with `staleTime: 0` (that pattern was burning IPC while keep-alive tool
 * hosts / ContextBar stayed mounted). Invalidate via `invalidateGitQueries`
 * after turns / cwd changes. */
export const useIsGitRepo = (cwd: string | undefined) =>
  useQuery({
    queryKey: ["git-is-repo", cwd ?? ""],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 60_000,
    refetchOnMount: true,
    refetchOnWindowFocus: true,
    refetchInterval: 60_000,
  })
