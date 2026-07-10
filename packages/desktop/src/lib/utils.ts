import {
  File,
  FileCode2,
  FileImage,
  FileJson,
  FileText,
} from "lucide-react"

export const cn = (...parts: (string | false | undefined | null)[]): string =>
  parts.filter(Boolean).join(" ")

export const formatRelativeTime = (tsMs: number): string => {
  const diff = Date.now() - tsMs
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return "just now"
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

/** Cursor-style turn duration: "12s", "1m 9s", "1h 2m". */
export const formatDuration = (ms: number): string => {
  const totalSeconds = Math.max(1, Math.round(ms / 1000))
  if (totalSeconds < 60) return `${totalSeconds}s`
  const totalMinutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  if (totalMinutes < 60) {
    return seconds > 0 ? `${totalMinutes}m ${seconds}s` : `${totalMinutes}m`
  }
  const hours = Math.floor(totalMinutes / 60)
  const minutes = totalMinutes % 60
  return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`
}

/** Compact token count: 840 → "840", 12_400 → "12.4k", 1_200_000 → "1.2M". */
export const formatTokens = (n: number): string => {
  if (n < 1000) return `${n}`
  if (n < 1_000_000) {
    const k = n / 1000
    return `${k >= 100 ? Math.round(k) : k.toFixed(1).replace(/\.0$/, "")}k`
  }
  const m = n / 1_000_000
  return `${m.toFixed(1).replace(/\.0$/, "")}M`
}

/** Turn cost as a short USD string: 0.0213 → "$0.02", 1.5 → "$1.50". */
export const formatCost = (usd: number): string => {
  if (usd > 0 && usd < 0.01) return "<$0.01"
  return `$${usd.toFixed(2)}`
}

/** Last path segment, tolerant of trailing slashes and Windows separators. */
export const basename = (path: string): string => {
  const trimmed = path.replace(/[/\\]+$/, "")
  const segment = trimmed.split(/[/\\]/).pop()
  return segment || trimmed || path
}

/** Compact Cursor-style relative time for sidebar rows. */
export const formatCompactTime = (tsMs: number): string => {
  const diff = Date.now() - tsMs
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return "now"
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}d`
  return `${Math.floor(days / 7)}w`
}

/** Lucide icon for a file path by extension (Changes panel). */
export const fileIconForPath = (path: string) => {
  const name = basename(path).toLowerCase()
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : ""
  if (
    ext === "ts" ||
    ext === "tsx" ||
    ext === "js" ||
    ext === "jsx" ||
    ext === "mjs" ||
    ext === "cjs"
  ) {
    return FileCode2
  }
  if (ext === "json" || ext === "jsonc") return FileJson
  if (
    ext === "png" ||
    ext === "jpg" ||
    ext === "jpeg" ||
    ext === "gif" ||
    ext === "webp" ||
    ext === "svg" ||
    ext === "ico"
  ) {
    return FileImage
  }
  if (ext === "css" || ext === "scss" || ext === "sass" || ext === "less") {
    return FileCode2
  }
  if (ext === "md" || ext === "mdx" || ext === "txt" || ext === "rst") {
    return FileText
  }
  if (ext === "html" || ext === "htm") return FileCode2
  if (name === "dockerfile" || ext === "toml" || ext === "yaml" || ext === "yml") {
    return FileCode2
  }
  return File
}
