import type { ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import type { SessionMeta } from "../lib/types"

export type UiPluginTab = {
  id: string
  label: string
  icon: LucideIcon
  enabled?: boolean
  render: (props: {
    active: boolean
    session: SessionMeta | undefined
  }) => ReactNode
}

export type UiMentionHit = {
  kind: "file" | "folder" | "table" | "mcp"
  name: string
  path: string
  insertText: string
}

export type UiMentionProvider = {
  id: string
  search: (query: string, cwd: string | undefined) => Promise<UiMentionHit[]>
}

export type UiPlugin = {
  id: string
  tabs?: UiPluginTab[]
  mentionProviders?: UiMentionProvider[]
  inlineCompletion?: boolean
}
