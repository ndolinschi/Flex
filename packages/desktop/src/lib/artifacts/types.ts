export type ArtifactKind =
  | "presentation"
  | "spreadsheet"
  | "csv"
  | "diagram"
  | "image"
  | "document"
  | "other"

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

export const inferArtifactKind = (path: string): ArtifactKind | null => {
  const lower = path.toLowerCase()
  const ext = lower.split(".").pop() ?? ""

  if (CODE_EXTS.has(ext)) return null

  if (ext === "csv" || ext === "tsv") return "csv"
  if (ext === "xlsx" || ext === "xls" || ext === "ods") return "spreadsheet"
  if (ext === "pptx" || ext === "ppt" || ext === "key") return "presentation"
  if (ext === "png" || ext === "jpg" || ext === "jpeg" || ext === "webp" || ext === "gif")
    return "image"
  if (ext === "svg" || ext === "mmd" || ext === "dot") return "diagram"
  if (ext === "pdf" || ext === "docx") return "document"

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
