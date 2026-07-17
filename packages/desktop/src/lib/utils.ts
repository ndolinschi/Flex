import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"
import {
  File,
  FileCode2,
  FileImage,
  FileJson,
  FileText,
} from "lucide-react"

/** Tailwind-aware class merger (shadcn convention). */
export const cn = (...inputs: ClassValue[]): string => twMerge(clsx(inputs))

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

/** turn duration: "12s", "1m 9s", "1h 2m". */
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

/** Parent path including the trailing separator, or "" when there is none.
 *  Tolerant of trailing slashes and Windows separators — pair with
 *  {@link basename} for muted-prefix + bright-name list rows. */
export const parentPathPrefix = (path: string): string => {
  const trimmed = path.replace(/[/\\]+$/, "")
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"))
  return i >= 0 ? trimmed.slice(0, i + 1) : ""
}

const hasDriveLetter = (path: string): boolean => /^[a-zA-Z]:\//.test(path)

/** True when `path` looks absolute (POSIX `/…` or Windows `C:/…`). */
export const isAbsolutePath = (path: string): boolean => {
  const normalized = path.replace(/\\/g, "/")
  return normalized.startsWith("/") || hasDriveLetter(normalized)
}

/** Strip `cwd` from an absolute tool `file_path` so review/Files commands
 * get a repo-relative path. Write/Edit always record absolute paths; review
 * APIs historically required relative ones — isolation is irrelevant. */
export const toSessionRelativePath = (
  path: string,
  cwd: string | null | undefined,
): string => {
  const trimmed = path.trim()
  if (!trimmed) return trimmed
  const normalized = trimmed.replace(/\\/g, "/")
  if (!cwd) return normalized
  let root = cwd.replace(/\\/g, "/").replace(/\/+$/, "")
  if (!root) return normalized
  if (normalized === root) return ""
  const prefix = `${root}/`
  if (normalized.startsWith(prefix)) return normalized.slice(prefix.length)
  // Windows: drive-letter / path casing often differs between tool args and
  // SessionMeta.cwd.
  const lower = normalized.toLowerCase()
  const rootLower = root.toLowerCase()
  if (lower === rootLower) return ""
  const prefixLower = `${rootLower}/`
  if (lower.startsWith(prefixLower)) {
    return normalized.slice(prefix.length)
  }
  return normalized
}

/** Compact relative time for sidebar rows. */
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

/** Countdown label for a memory expiry pill: "expires in 6d" / "expires in
 * 3h" / "expires in 12m", or "expired" once past. Deliberately coarse (one
 * unit, no combining) to stay compact at 11px in a list row. */
export const formatCountdown = (expiresAtMs: number, now: number = Date.now()): string => {
  const diff = expiresAtMs - now
  if (diff <= 0) return "expired"
  const minutes = Math.ceil(diff / 60_000)
  if (minutes < 60) return `expires in ${minutes}m`
  const hours = Math.ceil(minutes / 60)
  if (hours < 24) return `expires in ${hours}h`
  const days = Math.ceil(hours / 24)
  return `expires in ${days}d`
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
