/** Components Design Mode helpers: style-edit payload + composer merge. */

export type ComponentStyleChange = {
  property: string
  from: string
  to: string
}

export type ComponentStyleEditPayload = {
  componentName: string
  file: string
  exportName: string
  /** CSS selector when a live Design Mode / Browser target is known. */
  targetSelector?: string | null
  propsSummary?: string[]
  changes: ComponentStyleChange[]
}

const STYLE_EDIT_HEADING = "## Component style edit"
const STYLE_EDIT_SEPARATOR = "\n\n---\n\n"

/** Serialize a Components-tab CSS save into markdown for the agent. */
export const formatComponentStyleMarkdown = (
  payload: ComponentStyleEditPayload,
): string => {
  const lines: string[] = [
    STYLE_EDIT_HEADING,
    `- Component: ${payload.componentName} (${payload.file})`,
    `- Export: \`${payload.exportName}\``,
  ]
  if (payload.targetSelector) {
    lines.push(`- Target: \`${payload.targetSelector}\``)
  }
  if (payload.propsSummary && payload.propsSummary.length > 0) {
    lines.push(`- Props: ${payload.propsSummary.join(", ")}`)
  }
  lines.push("- Changes:")
  for (const change of payload.changes) {
    lines.push(`  - ${change.property}: ${change.from || "(unset)"} → ${change.to}`)
  }
  lines.push("")
  lines.push(
    "Please update the source so these styles apply (CSS module / Tailwind / styled / inline — whichever the component already uses).",
  )
  return lines.join("\n")
}

/** Merge style-edit context with the user's typed instruction. */
export const mergeComponentStyleWithDraft = (
  draft: string,
  payloads: ComponentStyleEditPayload[],
): string => {
  if (payloads.length === 0) return draft.trim()
  const context = payloads.map(formatComponentStyleMarkdown).join("\n\n")
  const text = draft.trim()
  if (!text) return context
  return `${context}${STYLE_EDIT_SEPARATOR}${text}`
}

/** Reverse of merge for DISPLAY: strip injected style-edit blocks from timeline. */
export const parseComponentStyleMessage = (
  text: string,
): { instruction: string; editCount: number } | null => {
  if (!text.startsWith(STYLE_EDIT_HEADING)) return null
  const sepIndex = text.indexOf(STYLE_EDIT_SEPARATOR)
  const context = sepIndex === -1 ? text : text.slice(0, sepIndex)
  const instruction =
    sepIndex === -1 ? "" : text.slice(sepIndex + STYLE_EDIT_SEPARATOR.length)
  const editCount = (context.match(/^## Component style edit/gm) ?? []).length
  return { instruction: instruction.trim(), editCount: Math.max(editCount, 1) }
}

/** CSS properties editable in the Components panel (subset aligned with Design Mode). */
export const COMPONENT_CSS_PROPERTIES = [
  "color",
  "background-color",
  "font-size",
  "font-weight",
  "font-family",
  "line-height",
  "padding",
  "margin",
  "display",
  "width",
  "height",
  "border-radius",
  "border",
  "opacity",
  "gap",
] as const

export type ComponentCssProperty = (typeof COMPONENT_CSS_PROPERTIES)[number]
