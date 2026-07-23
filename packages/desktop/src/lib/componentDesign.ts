
export type ComponentStyleChange = {
  property: string
  from: string
  to: string
}

export type ComponentStyleEditPayload = {
  componentName: string
  file: string
  exportName: string
  targetSelector?: string | null
  propsSummary?: string[]
  dependencies?: string[]
  sourceSnippet?: string | null
  changes: ComponentStyleChange[]
}

const STYLE_EDIT_HEADING = "## Component style edit"
const STYLE_EDIT_SEPARATOR = "\n\n---\n\n"

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
  if (payload.dependencies && payload.dependencies.length > 0) {
    lines.push(`- Dependencies: ${payload.dependencies.join(", ")}`)
  }
  if (payload.changes.length > 0) {
    lines.push("- Style changes:")
    for (const change of payload.changes) {
      lines.push(
        `  - ${change.property}: ${change.from || "(unset)"} → ${change.to}`,
      )
    }
  } else {
    lines.push("- Style changes: (none — follow the instruction only)")
  }
  if (payload.sourceSnippet?.trim()) {
    lines.push("")
    lines.push("### Source excerpt")
    lines.push("```tsx")
    lines.push(payload.sourceSnippet.trim())
    lines.push("```")
  }
  lines.push("")
  lines.push(
    "Update the source so the instruction and any style changes apply (CSS module / Tailwind / styled / inline — whichever the component already uses). Prefer editing this component and its listed dependencies only.",
  )
  return lines.join("\n")
}

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

export const diffStyleDrafts = (
  baseline: Record<string, string>,
  draft: Record<string, string>,
): ComponentStyleChange[] => {
  const changes: ComponentStyleChange[] = []
  const keys = new Set([...Object.keys(baseline), ...Object.keys(draft)])
  for (const property of keys) {
    const from = baseline[property] ?? ""
    const to = draft[property] ?? ""
    if (from.trim() === to.trim()) continue
    if (!to.trim() && !from.trim()) continue
    changes.push({ property, from, to })
  }
  return changes
}

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
