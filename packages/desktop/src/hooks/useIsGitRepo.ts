import { useQuery } from "@tanstack/react-query"
import { gitIsRepo } from "../lib/tauri"

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
