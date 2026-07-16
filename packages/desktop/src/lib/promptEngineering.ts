/** Prompt review helpers — map model findings onto character spans. */

import type { PromptReviewFinding } from "./tauri"

export type PromptSeverity = "error" | "warn" | "info"

export type PromptAnnotation = {
  start: number
  end: number
  severity: PromptSeverity
  message: string
  fix?: string
  quote: string
}

export type PromptSegment =
  | { kind: "text"; value: string }
  | {
      kind: "mark"
      value: string
      severity: PromptSeverity
      message: string
      fix?: string
    }

const normalizeSeverity = (raw: string): PromptSeverity => {
  const s = raw.toLowerCase()
  if (s === "error") return "error"
  if (s === "info") return "info"
  return "warn"
}

/** Locate each finding's quote in `draft` (first unused occurrence). */
export const annotationsFromFindings = (
  draft: string,
  findings: PromptReviewFinding[],
): PromptAnnotation[] => {
  const used: { start: number; end: number }[] = []
  const out: PromptAnnotation[] = []

  for (const f of findings) {
    const quote = f.quote
    if (!quote) continue
    let from = 0
    let start = -1
    while (from <= draft.length) {
      const idx = draft.indexOf(quote, from)
      if (idx < 0) break
      const end = idx + quote.length
      const overlaps = used.some((u) => idx < u.end && end > u.start)
      if (!overlaps) {
        start = idx
        used.push({ start: idx, end })
        break
      }
      from = idx + 1
    }
    if (start < 0) continue
    out.push({
      start,
      end: start + quote.length,
      severity: normalizeSeverity(f.severity),
      message: f.message,
      fix: f.fix?.trim() ? f.fix.trim() : undefined,
      quote,
    })
  }

  return out.sort((a, b) => a.start - b.start || b.end - a.end)
}

/** Split draft into plain + marked segments for the review view. */
export const segmentAnnotatedPrompt = (
  draft: string,
  annotations: PromptAnnotation[],
): PromptSegment[] => {
  if (!draft) return [{ kind: "text", value: "" }]
  if (annotations.length === 0) return [{ kind: "text", value: draft }]

  // Greedy non-overlapping: keep earliest, skip overlaps.
  const picked: PromptAnnotation[] = []
  for (const a of annotations) {
    if (picked.some((p) => a.start < p.end && a.end > p.start)) continue
    picked.push(a)
  }

  const segments: PromptSegment[] = []
  let cursor = 0
  for (const a of picked) {
    if (a.start > cursor) {
      segments.push({ kind: "text", value: draft.slice(cursor, a.start) })
    }
    segments.push({
      kind: "mark",
      value: draft.slice(a.start, a.end),
      severity: a.severity,
      message: a.message,
      fix: a.fix,
    })
    cursor = a.end
  }
  if (cursor < draft.length) {
    segments.push({ kind: "text", value: draft.slice(cursor) })
  }
  return segments
}

export type PromptSectionTemplate = {
  id: string
  label: string
  markdown: string
}

export const PROMPT_SECTION_TEMPLATES: PromptSectionTemplate[] = [
  {
    id: "structured",
    label: "Full structure",
    markdown: `## Goal

Describe the outcome you want.

## Constraints

- Must:
- Must not:

## Context

Relevant files, APIs, or prior decisions.

## Examples

Canonical input → expected behavior.

## Output

What the agent should produce.
`,
  },
  {
    id: "goal",
    label: "Goal",
    markdown: `## Goal

`,
  },
  {
    id: "constraints",
    label: "Constraints",
    markdown: `## Constraints

- Must:
- Must not:
`,
  },
]

export const appendPromptSection = (
  draft: string,
  sectionMarkdown: string,
): string => {
  const base = draft.replace(/\s+$/, "")
  if (!base) return sectionMarkdown
  return `${base}\n\n${sectionMarkdown}`
}

/** Rough token estimate (~4 chars/token). */
export const estimateTokens = (text: string): number => {
  const trimmed = text.trim()
  if (!trimmed) return 0
  return Math.max(1, Math.ceil(trimmed.length / 4))
}
