import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  getInlineCompletionPrefs,
  saveInlineCompletionPrefs,
  toInvokeError,
} from "../lib/tauri"
import type { InlineCompletionPrefs } from "../lib/types"
import { INLINE_COMPLETION_ENABLED } from "../lib/featureFlags"

export const INLINE_COMPLETION_PREFS_KEY = ["inline-completion-prefs"] as const

/** Shared TanStack query for desktop inline-completion prefs. */
export const useInlineCompletionPrefs = () => {
  const queryClient = useQueryClient()
  const enabled = INLINE_COMPLETION_ENABLED

  const query = useQuery({
    queryKey: INLINE_COMPLETION_PREFS_KEY,
    queryFn: getInlineCompletionPrefs,
    enabled,
    staleTime: 30_000,
  })

  const save = useMutation({
    mutationFn: (prefs: InlineCompletionPrefs) => saveInlineCompletionPrefs(prefs),
    onSuccess: (next) => {
      queryClient.setQueryData(INLINE_COMPLETION_PREFS_KEY, next)
    },
  })

  return {
    prefs: query.data,
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error ? toInvokeError(query.error) : null,
    save: save.mutateAsync,
    isSaving: save.isPending,
    refetch: query.refetch,
  }
}
