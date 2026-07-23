
import type { QueryClient } from "@tanstack/react-query"
import type { SessionEvent, SessionMeta } from "../types"
import { useAppStore } from "../../stores/appStore"
import { inferArtifactKind } from "../artifacts/types"
import { pathFromInput } from "../toolPresentation"
import { toSessionRelativePath } from "../utils"
import { artifactsRegister } from "../tauri"

const WRITE_TOOLS = new Set([
  "write",
  "edit",
  "multiedit",
  "createdocument",
  "createspreadsheet",
  "createpresentation",
])

const artifactTabOpenedForTurn = new Map<string, string>()

const sessionMetaFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): SessionMeta | undefined => {
  const sessions = queryClient?.getQueryData<SessionMeta[]>(["sessions"])
  return sessions?.find((s) => s.id === sessionId)
}

export const maybeRegisterArtifact = (
  event: SessionEvent,
  opts?: {
    activeSessionId?: string | null
    queryClient?: QueryClient
  },
): void => {
  const { payload } = event

  if (payload.kind !== "tool_call_updated") return
  const { call } = payload

  if (call.status.state !== "completed") return
  const toolName = call.tool_name.toLowerCase()
  if (!WRITE_TOOLS.has(toolName)) return

  const rawPath = pathFromInput(call.input)
  if (!rawPath) return

  const store = useAppStore.getState()
  const session = sessionMetaFromCache(opts?.queryClient, event.session_id)
  const cwd = session?.cwd

  const relativePath = toSessionRelativePath(rawPath, cwd)
  if (!relativePath) return

  const kind = inferArtifactKind(relativePath)
  if (!kind) return

  const projectKey = cwd?.trim()
  if (!projectKey) return

  void artifactsRegister(projectKey, event.session_id, relativePath).then(() => {
    const sessionId = event.session_id
    const turnKey = `${sessionId}:${call.id}`
    if (artifactTabOpenedForTurn.get(sessionId) === turnKey) return
    artifactTabOpenedForTurn.set(sessionId, turnKey)

    void opts?.queryClient?.invalidateQueries({
      queryKey: ["artifacts", projectKey],
    })

    const activeId = opts?.activeSessionId ?? store.activeSessionId
    if (sessionId !== activeId) return

    store.openToolBesideChat(sessionId, "artifacts")
  })
}
