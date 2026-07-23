import type { LucideIcon } from "lucide-react"
import {
  ExternalLink,
  FileCode2,
  FolderOpen,
  FolderTree,
  Globe,
  Package,
} from "lucide-react"

import type { ContextMenuItem } from "../../components/molecules/ContextMenu"
import { browserNavigate, browserOpen, artifactsOpenExternal } from "../tauri"
import { isAbsolutePath } from "../utils"
import { sessionScopeKey, useAppStore } from "../../stores/appStore"
import type { ArtifactKind } from "./types"

export type ArtifactOpenWithContext = {
  sessionId: string | null
  /** Session cwd / project key (absolute). */
  cwd: string | null | undefined
  relativePath: string
  /** Registered artifact id when known (Artifacts shelf). */
  artifactId?: string
  kind?: ArtifactKind | null
}

export type ArtifactOpenWithId =
  | "artifacts"
  | "file"
  | "files"
  | "folder"
  | "external"
  | "browser"

type TargetDef = {
  id: ArtifactOpenWithId
  label: string
  icon: LucideIcon
}

const TARGETS: TargetDef[] = [
  { id: "artifacts", label: "Open in Artifacts", icon: Package },
  { id: "file", label: "Open as file tab", icon: FileCode2 },
  { id: "files", label: "Reveal in Files", icon: FolderTree },
  { id: "folder", label: "Show in Folder", icon: FolderOpen },
  { id: "external", label: "Open externally", icon: ExternalLink },
  { id: "browser", label: "Open in Browser", icon: Globe },
]

export const toAbsoluteWorkspacePath = (
  cwd: string,
  relativePath: string,
): string => {
  const root = cwd.replace(/\\/g, "/").replace(/\/+$/, "")
  const rel = relativePath.trim().replace(/\\/g, "/")
  if (!rel) return root
  if (isAbsolutePath(rel)) return rel
  return `${root}/${rel.replace(/^\/+/, "")}`
}

export const isBrowserableArtifactPath = (path: string): boolean => {
  const ext = path.toLowerCase().split(".").pop() ?? ""
  return ext === "html" || ext === "htm"
}

/** Targets that have a real backend for this context. */
export const availableArtifactOpenWithIds = (
  ctx: ArtifactOpenWithContext,
): ArtifactOpenWithId[] => {
  const path = ctx.relativePath.trim().replace(/\\/g, "/")
  if (!path || path.endsWith("/")) return []

  const hasSession = !!ctx.sessionId
  const hasCwd = !!ctx.cwd?.trim()
  const ids: ArtifactOpenWithId[] = []

  if (hasSession) ids.push("artifacts")
  if (hasSession) ids.push("file")
  if (hasSession) ids.push("files")
  if (hasCwd) ids.push("folder")
  if (hasCwd && (ctx.artifactId || path)) ids.push("external")
  if (hasSession && hasCwd && isBrowserableArtifactPath(path)) {
    ids.push("browser")
  }

  return ids
}

export const buildArtifactOpenWithMenuItems = (
  ctx: ArtifactOpenWithContext,
  onError?: (message: string) => void,
): ContextMenuItem[] => {
  const ids = availableArtifactOpenWithIds(ctx)
  return ids.map((id) => {
    const def = TARGETS.find((t) => t.id === id)!
    return {
      type: "item" as const,
      label: def.label,
      icon: def.icon,
      onSelect: () => {
        void runArtifactOpenWith(id, ctx).catch((err) => {
          const message =
            err instanceof Error ? err.message : String(err ?? "Open failed")
          onError?.(message)
        })
      },
    }
  })
}

const openInBrowser = async (
  sessionId: string,
  url: string,
): Promise<void> => {
  const store = useAppStore.getState()
  const sessionKey = sessionScopeKey(sessionId)
  const wasStarted = !!store.browserBySession[sessionKey]?.started
  store.setBrowserSessionState(sessionKey, { loading: true, url })
  store.setBrowserOwnerSessionId(sessionKey)
  store.openToolBesideChat(sessionId, "browser")
  if (wasStarted) {
    await browserNavigate(url)
  } else {
    await browserOpen(url)
  }
}

export const runArtifactOpenWith = async (
  id: ArtifactOpenWithId,
  ctx: ArtifactOpenWithContext,
): Promise<void> => {
  const path = ctx.relativePath.trim().replace(/\\/g, "/")
  if (!path) return

  const store = useAppStore.getState()
  const sessionId = ctx.sessionId
  const cwd = ctx.cwd?.trim() ?? ""
  const sessionKey = sessionId ? sessionScopeKey(sessionId) : "none"

  switch (id) {
    case "artifacts": {
      if (!sessionId) return
      store.setArtifactFocusPath(sessionKey, path)
      store.openToolBesideChat(sessionId, "artifacts")
      return
    }
    case "file": {
      if (!sessionId) return
      store.openWorkspaceFile(sessionKey, path)
      return
    }
    case "files": {
      if (!sessionId) return
      store.setActiveWorkspaceFile(sessionKey, path)
      store.openToolBesideChat(sessionId, "files")
      return
    }
    case "folder": {
      if (!cwd) return
      const abs = toAbsoluteWorkspacePath(cwd, path)
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener")
      await revealItemInDir(abs)
      return
    }
    case "external": {
      if (ctx.artifactId && cwd) {
        await artifactsOpenExternal(cwd, ctx.artifactId)
        return
      }
      if (!cwd) return
      const abs = toAbsoluteWorkspacePath(cwd, path)
      const { openPath } = await import("@tauri-apps/plugin-opener")
      await openPath(abs)
      return
    }
    case "browser": {
      if (!sessionId || !cwd) return
      const abs = toAbsoluteWorkspacePath(cwd, path)
      const url = abs.startsWith("file:")
        ? abs
        : `file://${abs.startsWith("/") ? abs : `/${abs}`}`
      await openInBrowser(sessionId, url)
      return
    }
  }
}
