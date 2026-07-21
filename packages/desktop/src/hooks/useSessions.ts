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

/** Insert/replace a session in the list cache so UI selection cannot race
 * ahead of `invalidateQueries` (sidebar heal would otherwise see a missing id
 * and call `setActiveSessionId(null)`, wiping the content pane to empty). */
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
    // Prefer cache for label consumers (ContentPane / timeline / terminal).
    // Always refetch when the query is in error (FAILURE B: a stuck error
    // status must not block later create/delete invalidation). Otherwise
    // only refetch when stale.
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
      state.closeChatTab(deletedId)
    },
    // If the engine delete fails, we intentionally do nothing else here —
    // no local cleanup, no cache mutation. The caller (handleDelete) surfaces
    // the error; the row stays exactly as it was so the user can retry.
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

  /**
   * New Agent: reuse an empty "New Agent" draft for the same
   * project instead of spawning another UUID-titled row.
   */
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
        // Select immediately so the click feels instant; resume in background.
        setActiveSessionId(draft.id, { panel: "closed" })
        setRoute("chat")
        void resumeSession(draft.id).catch(() => {
          // Session may already be warm — keep the local selection.
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

  // Stable identities so SessionListItem's memo survives status-poll parent
  // re-renders (git/workspace tick must not rebuild every row).
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
