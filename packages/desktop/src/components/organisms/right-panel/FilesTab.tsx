import { FolderTree } from "lucide-react"
import { EmptyState } from "../../molecules"
import { sessionScopeKey, useAppStore } from "../../../stores/appStore"
import { FileExplorer } from "./FileExplorer"
import type { SessionMeta } from "../../../lib/types"

type FilesTabProps = {
  active: boolean
  session: SessionMeta | undefined
}

/**
 * Files tool tab = workspace explorer only (Cursor Agents pattern).
 * Opening a file creates a dedicated `kind: "file"` document tab in the
 * content TabStrip via `openWorkspaceFile` → `openFileBesideChat`.
 */
export const FilesTab = ({ active: _active, session }: FilesTabProps) => {
  const activeSessionId = session?.id
  const sessionKey = sessionScopeKey(activeSessionId ?? null)
  const openWorkspaceFile = useAppStore((s) => s.openWorkspaceFile)
  const activePath = useAppStore(
    (s) => s.activeFileBySession[sessionKey] ?? null,
  )
  const cwd = session?.cwd ?? ""
  const fallbackCwd = session?.base_cwd

  if (!activeSessionId) {
    return (
      <EmptyState
        className="min-h-0 flex-1"
        title="Select a session"
        description="Select a session to browse files."
      />
    )
  }

  if (!cwd) {
    return (
      <EmptyState
        className="min-h-0 flex-1"
        icon={<FolderTree className="h-6 w-6" aria-hidden />}
        title="No project folder"
        description="Pick a working directory for this session to browse files."
      />
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <FileExplorer
        sessionId={activeSessionId}
        sessionKey={sessionKey}
        cwd={cwd}
        fallbackCwd={fallbackCwd}
        activePath={activePath ?? undefined}
        onOpenFile={(p) => openWorkspaceFile(sessionKey, p)}
      />
    </div>
  )
}
