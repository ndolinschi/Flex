import { useCallback } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  cancel,
  createSession,
  deleteSession,
  listSessions,
  resumeSession,
  toInvokeError,
  updateSession,
} from "../lib/tauri"
import {
  findDraftSession,
  newAgentCreateInput,
  resolveCreateCwd,
} from "../lib/sessions"
import type { CreateSessionInput, SessionMeta, UpdateSessionInput } from "../lib/types"
import { persistUiState, useAppStore } from "../stores/appStore"

export const SESSIONS_KEY = ["sessions"] as const

export const upsertSessionInCache = (
  queryClient: ReturnType<typeof useQueryClient>,
  meta: SessionMeta,
): void => {
  queryClient.setQueryData<SessionMeta[]>(SESSIONS_KEY, (prev) => {
    if (!prev) return [meta]
    const idx = prev.findIndex((s) => s.id === meta.id)
    if (idx === -1) return [meta, ...prev]
    const next = prev.slice()
    next[idx] = meta
    return next
  })
}

export const useSessions = () => {
  const queryClient = useQueryClient()
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)

  const query = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    retry: 1,
    staleTime: 30_000,
    refetchOnMount: (q) =>
      q.state.status === "error" ? "always" : q.isStale(),
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateSessionInput) => createSession(input),
    onSuccess: (meta: SessionMeta) => {
      upsertSessionInCache(queryClient, meta)
      setActiveSessionId(meta.id, { panel: "closed" })
      setRoute("chat")
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
    },
  })

  const renameMutation = useMutation({
    mutationFn: ({ id, title }: { id: string; title: string }) =>
      updateSession(id, { title } satisfies UpdateSessionInput),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
    },
  })

  const deleteMutation = useMutation({
    mutationFn: async (id: string) => {
      try {
        await cancel(id)
      } catch {
      }
      await deleteSession(id)
    },
    onSuccess: (_data, deletedId) => {
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      const state = useAppStore.getState()
      if (state.activeSessionId === deletedId) {
        setActiveSessionId(null)
        void persistUiState({ activeSessionId: null })
      }
      if (state.pendingPermission?.sessionId === deletedId) {
        state.setPendingPermission(null)
      }
      if (state.pendingQuestion?.sessionId === deletedId) {
        state.setPendingQuestion(null)
      }
      state.clearSessionPanelState(deletedId)
      state.closeChatTab(deletedId)
    },
  })

  const createAsync = createMutation.mutateAsync
  const renameAsync = renameMutation.mutateAsync
  const deleteAsync = deleteMutation.mutateAsync

  const handleCreate = useCallback(
    async (input: CreateSessionInput = {}): Promise<SessionMeta> => {
      try {
        return await createAsync(input)
      } catch (err) {
        throw new Error(toInvokeError(err))
      }
    },
    [createAsync],
  )

  const handleNewAgent = useCallback(
    async (explicitCwd?: string): Promise<SessionMeta> => {
      const state = useAppStore.getState()
      const sessions = query.data ?? []
      const cwd = resolveCreateCwd(
        sessions,
        state.activeSessionId,
        state.recentCwds,
        explicitCwd,
      )
      const draft = findDraftSession(sessions, cwd)
      if (draft) {
        setActiveSessionId(draft.id, { panel: "closed" })
        setRoute("chat")
        void resumeSession(draft.id).catch(() => {
        })
        return draft
      }
      return handleCreate(
        newAgentCreateInput(
          cwd,
          state.selectedModelId,
          state.selectedIsolation,
          state.selectedReuseWorkspaceId,
        ),
      )
    },
    [handleCreate, query.data, setActiveSessionId, setRoute],
  )

  const handleRename = useCallback(
    async (id: string, title: string) => {
      try {
        await renameAsync({ id, title })
      } catch (err) {
        throw new Error(toInvokeError(err))
      }
    },
    [renameAsync],
  )

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteAsync(id)
      } catch (err) {
        throw new Error(toInvokeError(err))
      }
    },
    [deleteAsync],
  )

  return {
    sessions: query.data ?? [],
    isLoading: query.isLoading,
    isFetching: query.isFetching,
    isError: query.isError,
    error: query.error ? toInvokeError(query.error) : null,
    refetch: query.refetch,
    createSession: handleCreate,
    newAgent: handleNewAgent,
    renameSession: handleRename,
    deleteSession: handleDelete,
    isCreating: createMutation.isPending,
    isRenaming: renameMutation.isPending,
    isDeleting: deleteMutation.isPending,
  }
}
