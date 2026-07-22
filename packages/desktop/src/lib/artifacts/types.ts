/** AI-created non-code deliverable kinds. */
export type ArtifactKind =
  | "presentation"
  | "spreadsheet"
  | "csv"
  | "diagram"
  | "image"
  | "document"
  | "other"

/** A registered project deliverable created by an agent. */
export type Artifact = {
  id: string
  projectKey: string
  sessionId: string
  kind: ArtifactKind
  relativePath: string
  title: string
  createdAt: string
  mimeType?: string
}

export type CsvPreview = {
  columns: string[]
  rows: string[][]
  truncated: boolean
  rowCount: number
}

// ── Kind inference ────────────────────────────────────────────────────────────

/** Extensions that unambiguously belong to source code / config and must never
 *  be auto-registered as artifacts regardless of their parent directory. */
const CODE_EXTS = new Set([
  "ts",
  "tsx",
  "js",
  "jsx",
  "mjs",
  "cjs",
  "rs",
  "py",
  "go",
  "java",
  "kt",
  "swift",
  "c",
  "cpp",
  "h",
  "hpp",
  "cs",
  "rb",
  "php",
  "sh",
  "bash",
  "zsh",
  "fish",
  "ps1",
  "toml",
  "lock",
  "mod",
  "sum",
  "gradle",
  "cmake",
  "makefile",
  "dockerfile",
  "gitignore",
  "npmrc",
  "prettierrc",
  "eslintrc",
  "babelrc",
  "editorconfig",
])

/**
 * Infer the artifact kind from a file path.
 *
 * Returns `null` for code / source-file extensions — those must never be
 * auto-registered. Returns a kind for known deliverable extensions.  For
 * generic extensions (md, txt, json…) inside well-known artifact directories
 * (`artifacts/`, `reports/`, `exports/`, `plans/`) the file is promoted to
 * `"document"`.
 */
export const inferArtifactKind = (path: string): ArtifactKind | null => {
  const lower = path.toLowerCase()
  const ext = lower.split(".").pop() ?? ""

  if (CODE_EXTS.has(ext)) return null

  // Hard-coded artifact extensions.
  if (ext === "csv" || ext === "tsv") return "csv"
  if (ext === "xlsx" || ext === "xls" || ext === "ods") return "spreadsheet"
  if (ext === "pptx" || ext === "ppt" || ext === "key") return "presentation"
  if (ext === "png" || ext === "jpg" || ext === "jpeg" || ext === "webp" || ext === "gif")
    return "image"
  if (ext === "svg" || ext === "mmd" || ext === "dot") return "diagram"
  if (ext === "pdf" || ext === "docx") return "document"

  // Generic text / data files: only promote when inside an artifact directory.
  const inArtifactDir =
    lower.includes("/artifacts/") ||
    lower.includes("/reports/") ||
    lower.includes("/exports/") ||
    lower.includes("/plans/") ||
    lower.startsWith("artifacts/") ||
    lower.startsWith("reports/") ||
    lower.startsWith("exports/") ||
    lower.startsWith("plans/")

  if (
    inArtifactDir &&
    (ext === "md" ||
      ext === "txt" ||
      ext === "html" ||
      ext === "htm" ||
      ext === "json" ||
      ext === "yaml" ||
      ext === "yml" ||
      ext === "xml")
  ) {
    return "document"
  }

  return null
}
