import type { FileHit } from "./types"

/** Sort dirs first, then case-insensitive name (VS Code explorer order). */
export const sortFileHits = (hits: FileHit[]): FileHit[] =>
  [...hits].sort((a, b) => {
    const aDir = !!a.is_dir
    const bDir = !!b.is_dir
    if (aDir !== bDir) return aDir ? -1 : 1
    const byName = a.name.localeCompare(b.name, undefined, {
      sensitivity: "base",
    })
    if (byName !== 0) return byName
    return a.path.localeCompare(b.path)
  })

/** Parent directory path ("" for workspace root children). */
export const parentDir = (path: string): string => {
  const i = path.lastIndexOf("/")
  return i >= 0 ? path.slice(0, i) : ""
}
