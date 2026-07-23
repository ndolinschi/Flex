import type { FileHit } from "./types"

export const sortFileHits = (hits: FileHit[]): FileHit[] =>
  [...hits].sort((a, b) => {
    const aDir = !!a.isDir
    const bDir = !!b.isDir
    if (aDir !== bDir) return aDir ? -1 : 1
    const byName = a.name.localeCompare(b.name, undefined, {
      sensitivity: "base",
    })
    if (byName !== 0) return byName
    return a.path.localeCompare(b.path)
  })

export const parentDir = (path: string): string => {
  const i = path.lastIndexOf("/")
  return i >= 0 ? path.slice(0, i) : ""
}
