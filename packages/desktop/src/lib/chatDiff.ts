/** Parse helpers for Cursor-style chat diff cards (markdown fences + tool rows). */

export type ChatDiffLineKind = "add" | "remove" | "context" | "hunk" | "meta"

export type ChatDiffLine = {
  kind: ChatDiffLineKind
  /** Display text with leading `+`/`-`/` ` stripped (headers keep their markers). */
  text: string
}

export type ParsedChatDiff = {
  path: string | null
  lines: ChatDiffLine[]
  added: number
  removed: number
}

export type FenceMeta = {
  /** True when the fence should render as a chat diff card. */
  isDiff: boolean
  /** Language token for non-diff fences (e.g. `ts`), or `diff`. */
  language: string | null
  /** Path from fence meta (`diff path`, `12:15:path`, …). */
  path: string | null
}

const FILE_HEADER_RE = /^diff --git a\/(.*) b\/(.*)$/
const NEW_PATH_RE = /^\+\+\+ (?:b\/)?(.+)$/
const OLD_PATH_RE = /^--- (?:a\/)?(.+)$/
/** Cursor-style citation: `12:15:path/to/file.ts` */
const CURSOR_PATH_RE = /^\d+:\d+:(.+)$/
/** `diff path/to/file` or `diff:path` */
const DIFF_PATH_RE = /^diff(?:\s+|:)(.+)$/i

const stripQuotes = (raw: string): string => {
  let p = raw.trim()
  if (p.startsWith('"') && p.endsWith('"')) p = p.slice(1, -1)
  if (p === "/dev/null") return ""
  return p
}

/** Basename for the card header; empty when no path. */
export const chatDiffBasename = (path: string | null | undefined): string => {
  if (!path) return ""
  const norm = path.replace(/\\/g, "/")
  const i = norm.lastIndexOf("/")
  return i >= 0 ? norm.slice(i + 1) : norm
}

/** Short extension badge (`TS`, `RS`, `MD`, …). */
export const chatDiffExtBadge = (path: string | null | undefined): string => {
  const base = chatDiffBasename(path)
  if (!base) return "FILE"
  const dot = base.lastIndexOf(".")
  if (dot < 0 || dot === base.length - 1) return "FILE"
  return base.slice(dot + 1).toUpperCase().slice(0, 4)
}

/**
 * Parse a fenced-code info / language token from react-markdown
 * (`language-(\S+)`). Supports `diff`, `diff path`, Cursor `12:15:path`,
 * and plain language ids.
 */
export const parseFenceMeta = (info: string | null | undefined): FenceMeta => {
  const raw = (info ?? "").trim()
  if (!raw) {
    return { isDiff: false, language: null, path: null }
  }

  const cursor = CURSOR_PATH_RE.exec(raw)
  if (cursor) {
    return { isDiff: false, language: null, path: stripQuotes(cursor[1]) }
  }

  const diffPath = DIFF_PATH_RE.exec(raw)
  if (diffPath) {
    return {
      isDiff: true,
      language: "diff",
      path: stripQuotes(diffPath[1]),
    }
  }

  if (raw.toLowerCase() === "diff") {
    return { isDiff: true, language: "diff", path: null }
  }

  // `ts:path/file.ts` / `typescript:src/foo.ts`
  const colon = raw.indexOf(":")
  if (colon > 0 && !/^\d+$/.test(raw.slice(0, colon))) {
    const lang = raw.slice(0, colon)
    const path = stripQuotes(raw.slice(colon + 1))
    if (path && !path.includes(" ")) {
      return {
        isDiff: lang.toLowerCase() === "diff",
        language: lang,
        path,
      }
    }
  }

  return { isDiff: false, language: raw, path: null }
}

/**
 * True when `text` looks like a unified or simple +/- dump worth rendering
 * as a chat diff card (stricter than "any line starts with +").
 */
export const looksLikeDiff = (text: string): boolean => {
  const lines = text.replace(/\r\n/g, "\n").split("\n")
  let plusMinus = 0
  for (const line of lines) {
    if (line.startsWith("diff --git ") || line.startsWith("@@ ")) return true
    if (line.startsWith("+++ ") || line.startsWith("--- ")) continue
    if (line.startsWith("+") || line.startsWith("-")) {
      plusMinus += 1
      if (plusMinus >= 2) return true
    }
  }
  return false
}

const countStats = (lines: ChatDiffLine[]): { added: number; removed: number } => {
  let added = 0
  let removed = 0
  for (const l of lines) {
    if (l.kind === "add") added += 1
    else if (l.kind === "remove") removed += 1
  }
  return { added, removed }
}

/**
 * Parse unified / simple +/- text into display lines for `ChatDiffCard`.
 * Skips `diff --git` / `index` noise; keeps `@@` as hunk headers; strips
 * leading markers from content lines for display.
 */
export const parseChatDiff = (text: string): ParsedChatDiff => {
  const rawLines = text.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n")
  const lines: ChatDiffLine[] = []
  let path: string | null = null

  for (const line of rawLines) {
    const fileHeader = FILE_HEADER_RE.exec(line)
    if (fileHeader) {
      path = stripQuotes(fileHeader[2]) || stripQuotes(fileHeader[1]) || path
      continue
    }
    if (line.startsWith("index ") || line.startsWith("new file mode ") || line.startsWith("deleted file mode ")) {
      continue
    }
    if (line.startsWith("+++ ")) {
      const m = NEW_PATH_RE.exec(line)
      if (m) {
        const p = stripQuotes(m[1])
        if (p) path = p
      }
      continue
    }
    if (line.startsWith("--- ")) {
      // Prefer +++ path; only use --- if we have nothing yet.
      if (!path) {
        const m = OLD_PATH_RE.exec(line)
        if (m) {
          const p = stripQuotes(m[1])
          if (p) path = p
        }
      }
      continue
    }
    if (line.startsWith("@@")) {
      lines.push({ kind: "hunk", text: line })
      continue
    }
    if (line.startsWith("\\ ")) {
      lines.push({ kind: "meta", text: line })
      continue
    }
    if (line.startsWith("+")) {
      lines.push({ kind: "add", text: line.slice(1) })
      continue
    }
    if (line.startsWith("-")) {
      lines.push({ kind: "remove", text: line.slice(1) })
      continue
    }
    if (line.startsWith(" ")) {
      lines.push({ kind: "context", text: line.slice(1) })
      continue
    }
    // Unprefixed context (simple dumps without a leading space).
    if (line.length > 0 || lines.length > 0) {
      lines.push({ kind: "context", text: line })
    }
  }

  const { added, removed } = countStats(lines)
  return { path, lines, added, removed }
}

/** Whether a markdown code fence should become a ChatDiffCard. */
export const shouldRenderChatDiff = (
  language: string | null,
  body: string,
): boolean => {
  const meta = parseFenceMeta(language)
  if (meta.isDiff) return true
  return looksLikeDiff(body)
}
