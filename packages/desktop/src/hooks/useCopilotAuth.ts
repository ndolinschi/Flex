import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  copilotAuthCancel,
  copilotAuthStart,
  copilotAuthStatus,
  copilotAuthWait,
  toInvokeError,
} from "../lib/tauri"
import type { CopilotAuthStart } from "../lib/types"

const COPILOT_AUTH_KEY = ["copilot-auth-status"] as const

export const useCopilotAuth = (enabled = true) => {
  const queryClient = useQueryClient()

  const statusQuery = useQuery({
    queryKey: COPILOT_AUTH_KEY,
    queryFn: copilotAuthStatus,
    enabled,
    retry: 1,
    staleTime: 5_000,
  })

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: COPILOT_AUTH_KEY })
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
    mutationFn: () => wrap(() => copilotAuthStart()),
  })

  const waitMutation = useMutation({
    mutationFn: (sessionId: string) => wrap(() => copilotAuthWait(sessionId)),
    onSuccess: invalidate,
  })

  const cancelMutation = useMutation({
    mutationFn: (sessionId: string) => wrap(() => copilotAuthCancel(sessionId)),
  })

  return {
    signedIn: statusQuery.data?.signedIn ?? false,
    isLoading: statusQuery.isLoading,
    refetchStatus: statusQuery.refetch,
    start: (): Promise<CopilotAuthStart> => startMutation.mutateAsync(),
    wait: (sessionId: string) => waitMutation.mutateAsync(sessionId),
    cancel: (sessionId: string) => cancelMutation.mutateAsync(sessionId),
    isStarting: startMutation.isPending,
    isWaiting: waitMutation.isPending,
  }
}
