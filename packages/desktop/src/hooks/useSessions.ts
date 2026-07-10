import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
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
import { useAppStore } from "../stores/appStore"

const SESSIONS_KEY = ["sessions"] as const

export const useSessions = () => {
  const queryClient = useQueryClient()
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)

  const query = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    retry: 1,
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateSessionInput) => createSession(input),
    onSuccess: (meta: SessionMeta) => {
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      setActiveSessionId(meta.id)
      setRoute("chat")
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
    mutationFn: (id: string) => deleteSession(id),
    onSuccess: (_data, deletedId) => {
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      const activeId = useAppStore.getState().activeSessionId
      if (activeId === deletedId) {
        setActiveSessionId(null)
      }
    },
  })

  const handleCreate = async (
    input: CreateSessionInput = {},
  ): Promise<SessionMeta> => {
    try {
      return await createMutation.mutateAsync(input)
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  /**
   * Cursor-style New Agent: reuse an empty "New Agent" draft for the same
   * project instead of spawning another UUID-titled row.
   */
  const handleNewAgent = async (
    explicitCwd?: string,
  ): Promise<SessionMeta> => {
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
      try {
        await resumeSession(draft.id)
      } catch {
        // Still select locally if resume fails (session already warm).
      }
      setActiveSessionId(draft.id)
      setRoute("chat")
      return draft
    }
    return handleCreate(newAgentCreateInput(cwd, state.selectedModelId))
  }

  const handleRename = async (id: string, title: string) => {
    try {
      await renameMutation.mutateAsync({ id, title })
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id)
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  return {
    sessions: query.data ?? [],
    isLoading: query.isLoading,
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
