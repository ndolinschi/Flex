/** Design Mode (Cursor-style) helpers: DOM element payload + prompt serialization. */

export type BrowserDomRect = {
  x: number
  y: number
  width: number
  height: number
}

export type BrowserDomElement = {
  url: string
  tag: string
  id: string | null
  classes: string | null
  selector: string
  xpath: string
  attributes: Record<string, string>
  outerHtml: string
  styles: Record<string, string>
  rect: BrowserDomRect
}

export type BrowserDesignSelectEvent = {
  type: "select"
  additive: boolean
  name: string
  element: BrowserDomElement
}

export type BrowserDesignExitEvent = {
  type: "exit"
}

export type BrowserDesignEvent =
  | BrowserDesignSelectEvent
  | BrowserDesignExitEvent

/** Best-effort CSS selector for an element (mirrors the injected picker). */
export const buildCssSelector = (el: Element): string => {
  const owner = el.ownerDocument
  if (el.id && /^[A-Za-z][\w:-]*$/.test(el.id)) {
    return `#${cssEscape(el.id)}`
  }
  const testId = el.getAttribute("data-testid")
  if (testId) {
    return `[data-testid="${testId.replace(/"/g, '\\"')}"]`
  }
  const parts: string[] = []
  let cur: Element | null = el
  for (let depth = 0; cur && depth < 6; depth++) {
    if (cur === owner.body || cur === owner.documentElement) break
    const tag = cur.tagName.toLowerCase()
    if (cur.id && /^[A-Za-z][\w:-]*$/.test(cur.id)) {
      parts.unshift(`#${cssEscape(cur.id)}`)
      break
    }
    const curTag = cur.tagName
    const parent: Element | null = cur.parentElement
    if (!parent) {
      parts.unshift(tag)
      break
    }
    const siblings = Array.from(parent.children).filter(
      (c: Element) => c.tagName === curTag,
    )
    const idx = siblings.indexOf(cur) + 1
    parts.unshift(siblings.length > 1 ? `${tag}:nth-of-type(${idx})` : tag)
    cur = parent
  }
  return parts.join(" > ")
}

const cssEscape = (v: string): string => {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(v)
  }
  return v.replace(/[^a-zA-Z0-9_-]/g, "\\$&")
}

export const chipNameForElement = (el: Element): string => {
  const tag = el.tagName.toLowerCase()
  const aria = el.getAttribute("aria-label")
  if (aria) return `${tag} "${aria.slice(0, 40)}"`
  if (el.id) return `${tag}#${el.id}`
  const testId = el.getAttribute("data-testid")
  if (testId) return `${tag}[data-testid=${testId}]`
  const cls =
    typeof el.className === "string"
      ? el.className.trim().split(/\s+/)[0]
      : ""
  if (cls) return `${tag}.${cls}`
  return `<${tag}>`
}

const ATTR_KEYS = [
  "href",
  "name",
  "type",
  "role",
  "aria-label",
  "data-testid",
  "placeholder",
  "title",
  "alt",
  "for",
  "value",
] as const

const STYLE_KEYS = [
  "display",
  "color",
  "backgroundColor",
  "font",
  "width",
  "height",
  "padding",
  "margin",
] as const

/** Describe a live DOM element for Design Mode (preview iframe path). */
export const describeDomElement = (el: Element): BrowserDomElement => {
  const owner = el.ownerDocument
  const win = owner.defaultView
  const r = el.getBoundingClientRect()
  const attributes: Record<string, string> = {}
  for (const key of ATTR_KEYS) {
    const v = el.getAttribute(key)
    if (v != null && v !== "") attributes[key] = v.slice(0, 200)
  }
  const styles: Record<string, string> = {}
  if (win && "style" in el) {
    try {
      const cs = win.getComputedStyle(el as Element)
      for (const key of STYLE_KEYS) {
        styles[key] = cs[key as keyof CSSStyleDeclaration] as string
      }
    } catch {
      /* cross-realm / detached */
    }
  }
  let classes: string | null = null
  try {
    classes =
      typeof el.className === "string"
        ? el.className
        : ((el.className as { baseVal?: string } | undefined)?.baseVal ?? null)
  } catch {
    classes = null
  }
  let xpath = ""
  try {
    const parts: string[] = []
    let cur: Element | null = el
    const root = owner.documentElement
    while (cur && cur !== root) {
      const tag = cur.tagName.toLowerCase()
      const curTag = cur.tagName
      const parent: Element | null = cur.parentElement
      if (!parent) {
        parts.unshift(tag)
        break
      }
      const siblings = Array.from(parent.children).filter(
        (c: Element) => c.tagName === curTag,
      )
      const ix = siblings.indexOf(cur) + 1
      parts.unshift(siblings.length > 1 ? `${tag}[${ix}]` : tag)
      cur = parent
    }
    xpath = `/${parts.join("/")}`
  } catch {
    xpath = ""
  }
  let url = ""
  try {
    url = String(win?.location?.href || owner.URL || "")
  } catch {
    url = ""
  }
  return {
    url,
    tag: el.tagName.toLowerCase(),
    id: el.id || null,
    classes,
    selector: buildCssSelector(el),
    xpath,
    attributes,
    outerHtml: String(el.outerHTML || "").slice(0, 2000),
    styles,
    rect: { x: r.x, y: r.y, width: r.width, height: r.height },
  }
}

/** Serialize DOM chips into a markdown context block for the agent. */
export const formatDomContextMarkdown = (
  attachments: Array<{ name: string; payload: BrowserDomElement }>,
): string => {
  if (attachments.length === 0) return ""
  const blocks = attachments.map((att, i) => {
    const el = att.payload
    const lines: string[] = [
      `### Element ${i + 1}: \`${att.name}\``,
      `- URL: ${el.url || "(unknown)"}`,
      `- Selector: \`${el.selector || el.tag}\``,
    ]
    if (el.xpath) lines.push(`- XPath: \`${el.xpath}\``)
    if (el.tag) lines.push(`- Tag: \`${el.tag}\``)
    const attrEntries = Object.entries(el.attributes ?? {})
    if (attrEntries.length > 0) {
      lines.push(
        `- Attributes: ${attrEntries
          .map(([k, v]) => `${k}="${String(v).slice(0, 80)}"`)
          .join(", ")}`,
      )
    }
    const styleEntries = Object.entries(el.styles ?? {}).filter(
      ([, v]) => v && v !== "none" && v !== "normal",
    )
    if (styleEntries.length > 0) {
      lines.push(
        `- Styles: ${styleEntries
          .slice(0, 8)
          .map(([k, v]) => `${k}: ${v}`)
          .join("; ")}`,
      )
    }
    if (el.outerHtml?.trim()) {
      lines.push("")
      lines.push("```html")
      lines.push(el.outerHtml.trim())
      lines.push("```")
    }
    return lines.join("\n")
  })
  return [
    "## Selected page elements",
    "The user selected these elements in the embedded browser. Use them as the visual/DOM context for the request below.",
    "",
    ...blocks,
  ].join("\n")
}

const DOM_CONTEXT_HEADING = "## Selected page elements"
const DOM_CONTEXT_SEPARATOR = "\n\n---\n\n"

/** Reverse of {@link mergeDomContextWithDraft} for DISPLAY: a user message
 * whose text was built with a Design-Mode DOM-context block is shown in the
 * timeline as just the typed instruction plus a compact "N element(s)" chip —
 * never the raw injected markdown (which reads like a system prompt). Returns
 * `null` for ordinary messages (render them verbatim). The full context still
 * goes to the model unchanged; this only affects presentation. */
export const parseDomContextMessage = (
  text: string,
): { instruction: string; elementCount: number } | null => {
  if (!text.startsWith(DOM_CONTEXT_HEADING)) return null
  const sepIndex = text.indexOf(DOM_CONTEXT_SEPARATOR)
  const context = sepIndex === -1 ? text : text.slice(0, sepIndex)
  const instruction =
    sepIndex === -1 ? "" : text.slice(sepIndex + DOM_CONTEXT_SEPARATOR.length)
  const elementCount = (context.match(/^### Element \d+:/gm) ?? []).length
  return { instruction: instruction.trim(), elementCount: Math.max(elementCount, 1) }
}

/** Merge DOM context markdown with the user's typed instruction. */
export const mergeDomContextWithDraft = (
  draft: string,
  domAttachments: Array<{ name: string; payload: BrowserDomElement }>,
): string => {
  const context = formatDomContextMarkdown(domAttachments)
  const text = draft.trim()
  if (!context) return text
  if (!text) return context
  return `${context}\n\n---\n\n${text}`
}
