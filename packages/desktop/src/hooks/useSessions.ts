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

export const useSessions = () => {
  const queryClient = useQueryClient()
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)

  const query = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    retry: 1,
    // A prior failed mutation (e.g. a "not found" resume/delete) must not
    // leave this query permanently wedged in an error state — without this,
    // `invalidateQueries` after a later successful create/delete can no-op
    // against a query stuck on `status: "error"`, and the sidebar silently
    // never picks up new rows (see FAILURE B report).
    refetchOnMount: "always",
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateSessionInput) => createSession(input),
    onSuccess: (meta: SessionMeta) => {
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      setActiveSessionId(meta.id, { panel: "closed" })
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
    mutationFn: async (id: string) => {
      // Best-effort: the engine's own `delete_session` already cancels any
      // in-flight turn before deleting (see engine `Engine::delete_session`),
      // so this is a belt-and-suspenders nudge for a snappier UI stop, not a
      // correctness requirement. Never let a cancel failure block the delete.
      try {
        await cancel(id)
      } catch {
        // ignore — deletion proceeds regardless
      }
      await deleteSession(id)
    },
    onSuccess: (_data, deletedId) => {
      // Engine-side delete succeeded — local cleanup must ALWAYS run so a
      // rejected confirm/dialog or any earlier failure can never leave a
      // ghost row + dangling activeSessionId (see FAILURE A report).
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      const state = useAppStore.getState()
      if (state.activeSessionId === deletedId) {
        setActiveSessionId(null)
        // setActiveSessionId(null) already persists activeSessionId: null,
        // but do it explicitly too so a restart never resumes a dead id
        // even if that call path changes later.
        void persistUiState({ activeSessionId: null })
      }
      // A deleted session can never resolve its pending permission —
      // clear it so a stale modal can't outlive the session it belongs to.
      if (state.pendingPermission?.sessionId === deletedId) {
        state.setPendingPermission(null)
      }
      if (state.pendingQuestion?.sessionId === deletedId) {
        state.setPendingQuestion(null)
      }
      // Drop Files buffers / open tabs / terminal metas so deleted session
      // ids never leak into persisted openTabsBySession or in-memory drafts.
      state.clearSessionPanelState(deletedId)
    },
    // If the engine delete fails, we intentionally do nothing else here —
    // no local cleanup, no cache mutation. The caller (handleDelete) surfaces
    // the error; the row stays exactly as it was so the user can retry.
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
   * New Agent: reuse an empty "New Agent" draft for the same
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
      setActiveSessionId(draft.id, { panel: "closed" })
      setRoute("chat")
      return draft
    }
    return handleCreate(
      newAgentCreateInput(cwd, state.selectedModelId, state.selectedIsolation),
    )
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
