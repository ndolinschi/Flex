import { STATUS_COLOR } from "./FileRow"

export type GitStatusIndex = {
  byPath: Map<string, string>
  dirtyDirs: Set<string>
}

export const dirPrefix = (path: string): string => {
  const i = path.lastIndexOf("/")
  return i >= 0 ? path.slice(0, i + 1) : ""
}

export const isValidRelativeFilePath = (path: string): boolean => {
  const trimmed = path.trim().replace(/\\/g, "/")
  if (!trimmed || trimmed.endsWith("/")) return false
  if (trimmed.startsWith("/") || trimmed.includes("..")) return false
  return true
}

export const isValidBasename = (name: string): boolean => {
  const trimmed = name.trim()
  if (!trimmed) return false
  if (trimmed.includes("/") || trimmed.includes("\\") || trimmed.includes("..")) {
    return false
  }
  return true
}

/** Map git porcelain paths → status letter; also index dirty dir prefixes. */
export const buildGitStatusIndex = (
  files: ReadonlyArray<{ path: string; status: string }> | undefined,
): GitStatusIndex => {
  const byPath = new Map<string, string>()
  const dirtyDirs = new Set<string>()
  if (!files) return { byPath, dirtyDirs }
  for (const f of files) {
    const path = f.path.replace(/\\/g, "/")
    byPath.set(path, f.status)
    // Untracked dirs arrive with a trailing slash.
    if (path.endsWith("/")) {
      dirtyDirs.add(path.replace(/\/+$/, ""))
    }
    let rest = path.replace(/\/+$/, "")
    while (rest.includes("/")) {
      rest = rest.slice(0, rest.lastIndexOf("/"))
      if (!rest) break
      dirtyDirs.add(rest)
    }
  }
  return { byPath, dirtyDirs }
}

export const gitStatusClass = (
  path: string,
  isDir: boolean,
  index: GitStatusIndex,
): string | undefined => {
  const normalized = path.replace(/\\/g, "/")
  if (!isDir) {
    const status = index.byPath.get(normalized)
    return status ? (STATUS_COLOR[status] ?? undefined) : undefined
  }
  const dirStatus =
    index.byPath.get(normalized) ??
    index.byPath.get(`${normalized}/`) ??
    (index.dirtyDirs.has(normalized) ? "M" : undefined)
  return dirStatus ? (STATUS_COLOR[dirStatus] ?? undefined) : undefined
}
