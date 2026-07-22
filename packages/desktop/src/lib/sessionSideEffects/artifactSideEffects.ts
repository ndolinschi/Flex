/** Side-effects that auto-register AI-created artifacts when Write/Edit
 *  tool calls complete for artifact-extension file paths. */

import type { QueryClient } from "@tanstack/react-query"
import type { SessionEvent, SessionMeta } from "../types"
import { useAppStore } from "../../stores/appStore"
import { inferArtifactKind } from "../artifacts/types"
import { pathFromInput } from "../toolPresentation"
import { toSessionRelativePath } from "../utils"
import { artifactsRegister } from "../tauri"

/** Tool names that write files and may produce artifacts.
 *  Lowercase matches against `call.tool_name.toLowerCase()`. */
const WRITE_TOOLS = new Set([
  "write",
  "edit",
  "multiedit",
  "createdocument",
  "createspreadsheet",
  "createpresentation",
])

/** Per-session turn marker for which we already opened the Artifacts tab —
 *  prevents spamming the tab open on every Write in a single turn. */
const artifactTabOpenedForTurn = new Map<string, string>()

const sessionMetaFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): SessionMeta | undefined => {
  const sessions = queryClient?.getQueryData<SessionMeta[]>(["sessions"])
  return sessions?.find((s) => s.id === sessionId)
}

/** Call inside `applyGlobalSessionEvent` for live events only (not JSONL replay).
 *
 * When a Write/Edit tool call completes for an artifact-extension path,
 * this registers the artifact and opens the Artifacts tab beside chat
 * (once per turn per session). */
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

  // Only completed Write/Edit calls.
  if (call.status.state !== "completed") return
  const toolName = call.tool_name.toLowerCase()
  if (!WRITE_TOOLS.has(toolName)) return

  // Extract file path from the tool call input.
  const rawPath = pathFromInput(call.input)
  if (!rawPath) return

  const store = useAppStore.getState()
  const session = sessionMetaFromCache(opts?.queryClient, event.session_id)
  const cwd = session?.cwd

  const relativePath = toSessionRelativePath(rawPath, cwd)
  if (!relativePath) return

  // Only proceed if the file has an artifact extension.
  const kind = inferArtifactKind(relativePath)
  if (!kind) return

  const projectKey = cwd?.trim()
  if (!projectKey) return

  // Register asynchronously — fire-and-forget; errors are non-fatal.
  void artifactsRegister(projectKey, event.session_id, relativePath).then(() => {
    const sessionId = event.session_id
    const turnKey = `${sessionId}:${call.id}`
    if (artifactTabOpenedForTurn.get(sessionId) === turnKey) return
    artifactTabOpenedForTurn.set(sessionId, turnKey)

    const activeId = opts?.activeSessionId ?? store.activeSessionId
    if (sessionId !== activeId) return

    store.openToolBesideChat(sessionId, "artifacts")
  })
}
