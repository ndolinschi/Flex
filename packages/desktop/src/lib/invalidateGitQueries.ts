import type { QueryClient } from "@tanstack/react-query"

/**
 * Bust every Changes / ContextBar git observer. `git-status` alone is not
 * enough: the `git-is-repo` gate is a separate cache entry, and while it
 * stays `false` the status query is `enabled: false` — so a post-`git init`
 * tree never reappears until this key is invalidated too. `git-has-remote`
 * is included so adding/removing a remote updates Commit vs Commit & Push.
 */
export const invalidateGitQueries = (queryClient: QueryClient): void => {
  void queryClient.invalidateQueries({ queryKey: ["git-status"] })
  void queryClient.invalidateQueries({ queryKey: ["git-is-repo"] })
  void queryClient.invalidateQueries({ queryKey: ["git-has-remote"] })
  void queryClient.invalidateQueries({ queryKey: ["git-pr-status"] })
}
