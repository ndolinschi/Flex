
export type Hunk = {
  header: string
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  lines: string[]
}

export type ParsedDiffFile = {
  header: string[]
  oldPath: string | null
  newPath: string | null
  hunks: Hunk[]
}

export type ParsedDiff = {
  files: ParsedDiffFile[]
}

const TRUNCATION_MARKER = "… diff truncated …"

export const isDiffTruncated = (diff: string): boolean =>
  diff.includes(TRUNCATION_MARKER)

const FILE_HEADER_RE = /^diff --git a\/(.*) b\/(.*)$/
const OLD_PATH_RE = /^--- (?:a\/(.*)|"?(\/dev\/null)"?)/
const NEW_PATH_RE = /^\+\+\+ (?:b\/(.*)|"?(\/dev\/null)"?)/
const HUNK_HEADER_RE = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@.*$/

const stripPath = (raw: string): string => {
  let p = raw.trim()
  if (p.startsWith('"') && p.endsWith('"')) {
    p = p.slice(1, -1)
  }
  return p
}

export const parseUnifiedDiff = (text: string): ParsedDiff => {
  const files: ParsedDiffFile[] = []
  if (!text || !text.trim()) return { files }

  const rawLines = text.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n")

  let current: ParsedDiffFile | null = null
  let currentHunk: Hunk | null = null

  const pushCurrentHunk = () => {
    if (current && currentHunk) {
      current.hunks.push(currentHunk)
    }
    currentHunk = null
  }

  const pushCurrentFile = () => {
    pushCurrentHunk()
    if (current) files.push(current)
    current = null
  }

  for (let i = 0; i < rawLines.length; i++) {
    const line = rawLines[i]

    const fileHeaderMatch = FILE_HEADER_RE.exec(line)
    if (fileHeaderMatch) {
      pushCurrentFile()
      current = {
        header: [line],
        oldPath: stripPath(fileHeaderMatch[1]),
        newPath: stripPath(fileHeaderMatch[2]),
        hunks: [],
      }
      continue
    }

    const hunkHeaderMatch = HUNK_HEADER_RE.exec(line)
    if (hunkHeaderMatch) {
      if (!current) {
        current = { header: [], oldPath: null, newPath: null, hunks: [] }
      }
      pushCurrentHunk()
      const [, oldStart, oldLines, newStart, newLines] = hunkHeaderMatch
      currentHunk = {
        header: line,
        oldStart: Number(oldStart),
        oldLines: oldLines !== undefined ? Number(oldLines) : 1,
        newStart: Number(newStart),
        newLines: newLines !== undefined ? Number(newLines) : 1,
        lines: [],
      }
      continue
    }

    if (currentHunk) {
      currentHunk.lines.push(line)
      continue
    }

    if (!current) {
      current = { header: [], oldPath: null, newPath: null, hunks: [] }
    }
    current.header.push(line)

    const oldPathMatch = OLD_PATH_RE.exec(line)
    if (oldPathMatch) {
      current.oldPath = oldPathMatch[2] ? null : stripPath(oldPathMatch[1] ?? "")
      continue
    }
    const newPathMatch = NEW_PATH_RE.exec(line)
    if (newPathMatch) {
      current.newPath = newPathMatch[2] ? null : stripPath(newPathMatch[1] ?? "")
      continue
    }
  }

  pushCurrentFile()

  return { files }
}

export const buildPatch = (
  file: ParsedDiffFile,
  selectedHunks: Hunk[],
): string => {
  const parts: string[] = []

  if (file.header.length > 0) {
    parts.push(...file.header)
  } else {
    const a = file.oldPath ?? "/dev/null"
    const b = file.newPath ?? file.oldPath ?? "/dev/null"
    parts.push(`--- ${file.oldPath === null ? "/dev/null" : `a/${a}`}`)
    parts.push(`+++ ${file.newPath === null ? "/dev/null" : `b/${b}`}`)
  }

  for (const hunk of selectedHunks) {
    parts.push(hunk.header)
    parts.push(...hunk.lines)
  }

  const patch = parts.join("\n")
  return patch.endsWith("\n") ? patch : `${patch}\n`
}

export const describeHunklessDiff = (file: ParsedDiffFile): string => {
  const header = file.header.join("\n")
  if (header.includes("Binary files ") || /^GIT binary patch/m.test(header)) {
    return "Binary file"
  }
  if (header.includes("new file mode")) {
    return "Empty new file"
  }
  if (header.includes("deleted file mode")) {
    return "Empty deleted file"
  }
  if (
    header.includes("similarity index") ||
    /^rename from /m.test(header) ||
    /^rename to /m.test(header)
  ) {
    return "Renamed — no content change"
  }
  if (header.includes("old mode ") || header.includes("new mode ")) {
    return "Mode change only"
  }
  return "No content changes"
}

/** Soft-cap for rendered unified-diff lines (DOM cost), not parse. */
export const DIFF_RENDER_LINE_CAP = 800

export type SoftCappedLines<T> = {
  lines: T[]
  truncated: number
}

/**
 * Keep at most `cap` lines for render; callers show a muted footer when
 * `truncated > 0`. Parsing stays full-size.
 */
export const softCapLines = <T>(
  lines: readonly T[],
  cap: number = DIFF_RENDER_LINE_CAP,
): SoftCappedLines<T> => {
  if (lines.length <= cap) return { lines: lines as T[], truncated: 0 }
  return {
    lines: lines.slice(0, cap) as T[],
    truncated: lines.length - cap,
  }
}

/** New-file line count omitted before the first hunk (Cursor-style collapse). */
export const unmodifiedLinesBeforeHunk = (hunk: Hunk): number =>
  Math.max(0, hunk.newStart - 1)

/**
 * New-file lines between consecutive hunks (gap after `prev` ends and before
 * `next` starts). Negative/zero gaps return 0 — overlapping hunks are ignored.
 */
export const unmodifiedLinesBetweenHunks = (prev: Hunk, next: Hunk): number => {
  const prevEndExclusive = prev.newStart + prev.newLines
  return Math.max(0, next.newStart - prevEndExclusive)
}
