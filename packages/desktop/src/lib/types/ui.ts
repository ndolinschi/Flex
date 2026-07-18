import type { SessionMeta, ContentBlock } from "./wire"
import type { BrowserDomElement } from "../browserDesign"
import type { ComponentStyleEditPayload } from "../componentDesign"

export type ComposerMode = "agent" | "plan" | "ask" | "flex" | "debug"

export type FileComposerAttachment = {
  id: string
  path: string
  kind: "image" | "file" | "directory"
  name: string
}

export type DomComposerAttachment = {
  id: string
  kind: "dom"
  name: string
  payload: BrowserDomElement
}

export type ComponentStyleComposerAttachment = {
  id: string
  kind: "component-style"
  name: string
  payload: ComponentStyleEditPayload
}

export type ComposerAttachment =
  | FileComposerAttachment
  | DomComposerAttachment
  | ComponentStyleComposerAttachment

export const isFileAttachment = (
  att: ComposerAttachment,
): att is FileComposerAttachment =>
  att.kind === "image" || att.kind === "file" || att.kind === "directory"

export const isDomAttachment = (
  att: ComposerAttachment,
): att is DomComposerAttachment => att.kind === "dom"

export const isComponentStyleAttachment = (
  att: ComposerAttachment,
): att is ComponentStyleComposerAttachment => att.kind === "component-style"

export type AppRoute =
  | "chat"
  | "settings"
  | "customize"
  | "automations"
  | "memory"
  | "welcome"

/** Preset TTLs offered by the memory expiry menu, mapped to absolute
 * `expiresAtMs` at selection time. `"forever"` clears any expiry. */
export type MemoryTtlPreset = "forever" | "1d" | "1w" | "30d"

const MEMORY_TTL_MS: Record<Exclude<MemoryTtlPreset, "forever">, number> = {
  "1d": 24 * 60 * 60 * 1000,
  "1w": 7 * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000,
}

/** Absolute expiry timestamp for a TTL preset selected "now", or `undefined`
 * for `"forever"` (never expires). */
export const memoryExpiryFromPreset = (
  preset: MemoryTtlPreset,
  now: number = Date.now(),
): number | undefined => {
  if (preset === "forever") return undefined
  return now + MEMORY_TTL_MS[preset]
}

export const extractMarkdownText = (blocks: ContentBlock[]): string => {
  const parts: string[] = []
  for (const block of blocks) {
    if (block.type === "markdown") {
      parts.push(block.text)
    }
  }
  return parts.join("\n\n")
}

export const extractThinkingText = (blocks: ContentBlock[]): string => {
  const parts: string[] = []
  for (const block of blocks) {
    if (block.type === "thinking") {
      parts.push(block.text)
    }
  }
  return parts.join("\n\n")
}

/** True when a user_message should render as a chat bubble (not tool-result feedback). */
export const hasVisibleUserContent = (blocks: ContentBlock[]): boolean => {
  for (const block of blocks) {
    if (block.type === "markdown" && block.text.trim()) return true
    if (block.type === "image" || block.type === "file") return true
  }
  return false
}

export const DEFAULT_SESSION_TITLE = "New Agent"

export const truncateId = (id: string, len = 8): string => {
  if (id.length <= len) return id
  return `${id.slice(0, len)}…`
}

/** True when the session still has the placeholder title (or none). */
export const isDefaultSessionTitle = (title?: string | null): boolean => {
  const t = title?.trim()
  return !t || t === DEFAULT_SESSION_TITLE
}

/** Title derived from the first user prompt . */
export const titleFromPrompt = (text: string, maxLen = 48): string => {
  const cleaned = text.replace(/\s+/g, " ").trim()
  if (!cleaned) return DEFAULT_SESSION_TITLE
  if (cleaned.length <= maxLen) return cleaned
  const slice = cleaned.slice(0, maxLen)
  const lastSpace = slice.lastIndexOf(" ")
  const base = lastSpace > 16 ? slice.slice(0, lastSpace) : slice
  return `${base.trimEnd()}…`
}

export const sessionLabel = (meta: SessionMeta): string => {
  if (meta.title?.trim()) return meta.title.trim()
  return DEFAULT_SESSION_TITLE
}
