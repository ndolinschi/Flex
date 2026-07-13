import { useQuery } from "@tanstack/react-query"
import { gitIsRepo } from "../lib/tauri"

/** Shared `git-is-repo` observer — ContextBar, FilesChangedCard, RightPanel,
 * and ChangesTab all need the same gate. Identical queryKey + options so
 * TanStack Query dedupes the 5s refetch to one interval per cwd. */
export const useIsGitRepo = (cwd: string | undefined) =>
  useQuery({
    queryKey: ["git-is-repo", cwd ?? ""],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  })
