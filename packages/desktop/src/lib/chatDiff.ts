
export type ChatDiffLineKind = "add" | "remove" | "context" | "hunk" | "meta"

export type ChatDiffLine = {
  kind: ChatDiffLineKind
  text: string
}

export type ParsedChatDiff = {
  path: string | null
  lines: ChatDiffLine[]
  added: number
  removed: number
}

export type FenceMeta = {
  isDiff: boolean
  language: string | null
  path: string | null
}

const FILE_HEADER_RE = /^diff --git a\/(.*) b\/(.*)$/
const NEW_PATH_RE = /^\+\+\+ (?:b\/)?(.+)$/
const OLD_PATH_RE = /^--- (?:a\/)?(.+)$/
const CURSOR_PATH_RE = /^\d+:\d+:(.+)$/
const DIFF_PATH_RE = /^diff(?:\s+|:)(.+)$/i

const stripQuotes = (raw: string): string => {
  let p = raw.trim()
  if (p.startsWith('"') && p.endsWith('"')) p = p.slice(1, -1)
  if (p === "/dev/null") return ""
  return p
}

export const chatDiffBasename = (path: string | null | undefined): string => {
  if (!path) return ""
  const norm = path.replace(/\\/g, "/")
  const i = norm.lastIndexOf("/")
  return i >= 0 ? norm.slice(i + 1) : norm
}

export const chatDiffExtBadge = (path: string | null | undefined): string => {
  const base = chatDiffBasename(path)
  if (!base) return "FILE"
  const dot = base.lastIndexOf(".")
  if (dot < 0 || dot === base.length - 1) return "FILE"
  return base.slice(dot + 1).toUpperCase().slice(0, 4)
}

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
    if (line.length > 0 || lines.length > 0) {
      lines.push({ kind: "context", text: line })
    }
  }

  const { added, removed } = countStats(lines)
  return { path, lines, added, removed }
}

export const shouldRenderChatDiff = (
  language: string | null,
  body: string,
): boolean => {
  const meta = parseFenceMeta(language)
  if (meta.isDiff) return true
  return looksLikeDiff(body)
}
