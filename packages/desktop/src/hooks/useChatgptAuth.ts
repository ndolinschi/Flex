import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  chatgptAuthCancel,
  chatgptAuthStart,
  chatgptAuthStatus,
  chatgptAuthWait,
  toInvokeError,
} from "../lib/tauri"
import type { ChatgptAuthStart } from "../lib/types"

const CHATGPT_AUTH_KEY = ["chatgpt-auth-status"] as const

/** ChatGPT Plus/Pro headless OAuth status + start/wait/cancel. */
export const useChatgptAuth = (enabled = true) => {
  const queryClient = useQueryClient()

  const statusQuery = useQuery({
    queryKey: CHATGPT_AUTH_KEY,
    queryFn: chatgptAuthStatus,
    enabled,
    retry: 1,
    staleTime: 5_000,
  })

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: CHATGPT_AUTH_KEY })
    void queryClient.invalidateQueries({ queryKey: ["provider-profiles"] })
    void queryClient.invalidateQueries({ queryKey: ["provider-config"] })
    void queryClient.invalidateQueries({ queryKey: ["models"] })
  }

  const wrap = async <T,>(fn: () => Promise<T>): Promise<T> => {
    try {
      return await fn()
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  const startMutation = useMutation({
    mutationFn: () => wrap(() => chatgptAuthStart()),
  })

  const waitMutation = useMutation({
    mutationFn: (sessionId: string) => wrap(() => chatgptAuthWait(sessionId)),
    onSuccess: invalidate,
  })

  const cancelMutation = useMutation({
    mutationFn: (sessionId: string) => wrap(() => chatgptAuthCancel(sessionId)),
  })

  return {
    signedIn: statusQuery.data?.signedIn ?? false,
    isLoading: statusQuery.isLoading,
    refetchStatus: statusQuery.refetch,
    start: (): Promise<ChatgptAuthStart> => startMutation.mutateAsync(),
    wait: (sessionId: string) => waitMutation.mutateAsync(sessionId),
    cancel: (sessionId: string) => cancelMutation.mutateAsync(sessionId),
    isStarting: startMutation.isPending,
    isWaiting: waitMutation.isPending,
  }
}
