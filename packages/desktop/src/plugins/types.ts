import type { ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import type { SessionMeta } from "../lib/types"

/** A right-sidebar tab contributed by a UI plugin (not the agent engine). */
export type UiPluginTab = {
  id: string
  label: string
  icon: LucideIcon
  /** When false, the tab is omitted from the strip / "+" menu. Default true. */
  enabled?: boolean
  /** Render the tab body. `active` is true when this tab is selected and the panel is open. */
  render: (props: {
    active: boolean
    session: SessionMeta | undefined
  }) => ReactNode
}

/** Optional @-mention suggestion provider (files / folders / tables / MCP / …). */
export type UiMentionHit = {
  kind: "file" | "folder" | "table" | "mcp"
  name: string
  /** Secondary label (relative path, schema.table, “MCP server”, …). */
  path: string
  /** Opaque payload for the insert handler. */
  insertText: string
}

export type UiMentionProvider = {
  id: string
  search: (query: string, cwd: string | undefined) => Promise<UiMentionHit[]>
}

/** Desktop UI plugin — contributes chrome (tabs, mentions, inline completion),
 * not engine tools. */
export type UiPlugin = {
  id: string
  tabs?: UiPluginTab[]
  mentionProviders?: UiMentionProvider[]
  /** When true, this plugin contributes inline prompt ghost-text completion. */
  inlineCompletion?: boolean
}
