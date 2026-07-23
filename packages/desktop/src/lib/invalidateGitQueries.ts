import type { QueryClient } from "@tanstack/react-query"

export const invalidateGitQueries = (queryClient: QueryClient): void => {
  void queryClient.invalidateQueries({ queryKey: ["git-status"] })
  void queryClient.invalidateQueries({ queryKey: ["git-is-repo"] })
  void queryClient.invalidateQueries({ queryKey: ["git-has-remote"] })
  void queryClient.invalidateQueries({ queryKey: ["git-pr-status"] })
  void queryClient.invalidateQueries({ queryKey: ["git-pr-diff"] })
  void queryClient.invalidateQueries({ queryKey: ["git-pr-draft"] })
}
