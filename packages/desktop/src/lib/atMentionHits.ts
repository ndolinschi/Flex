import type { UiMentionHit } from "../plugins/types"

/** Unified @-mention row for the composer tray (files, folders, DB tables, MCP). */
export type AtMentionHit = {
  kind: "file" | "folder" | "table" | "mcp"
  name: string
  path: string
  /** Text inserted after `@`. Defaults to `name`. */
  insertText?: string
  /** Absolute/relative path for file/folder attachments. */
  attachPath?: string
}

export const fileHitToAtMention = (hit: {
  name: string
  path: string
  isDir?: boolean
}): AtMentionHit => ({
  kind: hit.isDir ? "folder" : "file",
  name: hit.name,
  path: hit.path,
  insertText: hit.name,
  attachPath: hit.path,
})

export const pluginHitToAtMention = (hit: UiMentionHit): AtMentionHit => ({
  kind: hit.kind,
  name: hit.name,
  path: hit.path,
  insertText: hit.insertText || hit.name,
})
