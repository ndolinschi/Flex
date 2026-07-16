// Unified-diff parser for the per-file / per-hunk review UI (Changes tab).
//
// Parses the plain-text output of `git diff` / `review_file_diff` into a
// structured shape so DiffView can render per-hunk Keep/Undo actions, and so
// individual hunks can be re-serialized into a small standalone patch that
// `review_apply_patch` can `git apply` (optionally `--reverse`) against the
// worktree or the isolated session's base repo.

/** One `@@ -a,b +c,d @@` hunk, including its header line. `lines` keep their
 * leading `+`/`-`/` ` prefix (and any trailing `\ No newline at end of file`
 * marker line), i.e. they are exactly the lines `git apply` expects between
 * hunk headers. */
export type Hunk = {
  header: string
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  lines: string[]
}

/** One file section of a unified diff: everything from `diff --git …` (or the
 * first `---`/`+++` pair for a `--no-index` diff) up to its hunks. */
export type ParsedDiffFile = {
  /** Raw header lines preceding the first hunk — `diff --git`, `index`,
   * `new file mode`, `--- `, `+++ `, etc. Kept verbatim so `buildPatch` can
   * reproduce a valid patch without re-deriving them. */
  header: string[]
  /** `a/`-relative path, or `null` for `/dev/null` (newly added file). */
  oldPath: string | null
  /** `b/`-relative path, or `null` for `/dev/null` (deleted file). */
  newPath: string | null
  hunks: Hunk[]
}

export type ParsedDiff = {
  files: ParsedDiffFile[]
}

/** The exact marker `truncate_diff` in `src-tauri/src/commands.rs` appends
 * when a diff exceeds `MAX_DIFF_BYTES` — the raw string is
 * `"\n… diff truncated …\n"`, so the text always contains this line. When
 * present, the diff was cut mid-stream (possibly mid-hunk), so we never treat
 * it as hunk-actionable — only the fallback plain-line rendering is safe. */
const TRUNCATION_MARKER = "… diff truncated …"

/** True when `diff` was cut short by the backend's byte cap. Callers should
 * fall back to plain (non-actionable) rendering in this case — a truncated
 * diff may have a partial final hunk that would produce a broken patch. */
export const isDiffTruncated = (diff: string): boolean =>
  diff.includes(TRUNCATION_MARKER)

const FILE_HEADER_RE = /^diff --git a\/(.*) b\/(.*)$/
const OLD_PATH_RE = /^--- (?:a\/(.*)|"?(\/dev\/null)"?)/
const NEW_PATH_RE = /^\+\+\+ (?:b\/(.*)|"?(\/dev\/null)"?)/
const HUNK_HEADER_RE = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@.*$/

/** Strip a trailing `\r` some diffs carry over CRLF-checked-out repos, and
 * unquote a `git diff` path that was quoted+escaped for special characters
 * (e.g. spaces or unicode) — rare, but cheap to defend against. */
const stripPath = (raw: string): string => {
  let p = raw.trim()
  if (p.startsWith('"') && p.endsWith('"')) {
    p = p.slice(1, -1)
  }
  return p
}

/**
 * Parse a unified diff (as produced by `git diff` / `review_file_diff`) into
 * structured file + hunk sections.
 *
 * Defensive by construction — there is no test runner configured for this
 * package, so parsing failures must degrade gracefully rather than throw:
 * any file/hunk that doesn't match the expected shape is simply omitted from
 * `files`/`hunks` rather than aborting the whole parse. Callers (DiffView)
 * treat an empty or partial result as "not hunk-actionable" and fall back to
 * plain line rendering.
 *
 * Handles:
 * - Multiple files in one diff (concatenated `diff --git` sections) — not
 *   currently produced by the backend (one path in, one diff out) but the
 *   parser doesn't assume a single file.
 * - `/dev/null` old/new paths (added/deleted files, and the `--no-index`
 *   fallback `diff_against_rev` uses for untracked files).
 * - `\ No newline at end of file` marker lines — kept as an ordinary line
 *   inside the enclosing hunk (it carries no +/-/space prefix of its own but
 *   `git apply` expects it immediately after the line it annotates).
 * - The backend's truncation marker (`isDiffTruncated`) — this function
 *   still parses whatever came before the marker, but callers must check
 *   `isDiffTruncated` themselves before trusting hunks as actionable, since
 *   the final hunk in a truncated diff may be incomplete.
 * - A diff with no `diff --git` line at all (e.g. a bare `--no-index`
 *   `---`/`+++` pair) — treated as a single implicit file.
 */
export const parseUnifiedDiff = (text: string): ParsedDiff => {
  const files: ParsedDiffFile[] = []
  if (!text || !text.trim()) return { files }

  // Normalize line endings; keep an empty trailing line out of the split so
  // we don't manufacture a phantom last "line" for diffs ending in \n.
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
      // No `diff --git` seen yet (bare --no-index diff) — open an implicit
      // file section so the hunk has somewhere to live.
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
      // Inside a hunk: content lines (+/-/space) and the "no newline" marker
      // all belong verbatim to the hunk body.
      currentHunk.lines.push(line)
      continue
    }

    // Outside any hunk: header material for the current (or not-yet-typed)
    // file — `index`, `new file mode`, `deleted file mode`, `---`, `+++`,
    // `Binary files … differ`, rename headers, etc.
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

/**
 * Reconstruct a standalone, `git apply`-able unified diff patch containing
 * only `selectedHunks` from `file`.
 *
 * Correctness note: hunk line-number offsets (the `-a,b +c,d` numbers in each
 * `@@` header) are NOT recalculated here, even when a subset of a file's
 * hunks is selected. This is safe because:
 * - Each hunk's header lines/offsets are only interpreted relative to the
 *   *original* file content (the `-` side), never relative to sibling
 *   hunks — `git apply` seeks to each hunk's own `oldStart` independently.
 * - We never split, merge, or edit the *content* of a hunk — hunks are
 *   passed through verbatim, in their original order — so each one is still
 *   an exact, valid diff of the base file at its stated line range.
 * - Applying a subset of independent, unmodified hunks against the same base
 *   content those hunks were computed from is exactly what `git apply`
 *   (and `patch`) support natively; only *editing* a hunk's line count
 *   without updating its header would require recalculating offsets, which
 *   this function deliberately never does.
 *
 * If hunks were reordered, merged, or had lines added/removed relative to
 * what `parseUnifiedDiff` produced, this assumption would break — callers
 * must pass hunks through unmodified and in ascending order.
 */
export const buildPatch = (
  file: ParsedDiffFile,
  selectedHunks: Hunk[],
): string => {
  const parts: string[] = []

  if (file.header.length > 0) {
    parts.push(...file.header)
  } else {
    // No captured header (bare --no-index diff, or a hunk-only fragment) —
    // synthesize a minimal valid header so `git apply` has a target path.
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

/**
 * Human-readable label for a file section that parsed cleanly but has no
 * hunks — empty adds (git's `e69de29` empty blob), binary notices, renames
 * with no content change, mode-only flips. DiffView uses this instead of
 * dumping raw `diff --git` / `index` metadata at the user.
 */
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
