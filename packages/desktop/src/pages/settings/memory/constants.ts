import type { MemoryEntryDto, MemoryTtlPreset } from "../../../lib/types"

export const MEMORY_KEY = ["memory"] as const
export const projectMemoryKey = (cwd: string) => ["project-memory", cwd] as const

export const EMPTY_MEMORIES: MemoryEntryDto[] = []

export type MemoryScope = {
  getMemory: (id: string) => Promise<MemoryEntryDto>
  removeMemory: (id: string) => Promise<void>
  setExpiry: (id: string, expiresAtMs: number | undefined) => Promise<void>
  invalidateKey: readonly unknown[]
}

export const TTL_PRESETS: { preset: MemoryTtlPreset; label: string }[] = [
  { preset: "forever", label: "Keep forever" },
  { preset: "1d", label: "1 day" },
  { preset: "1w", label: "1 week" },
  { preset: "30d", label: "30 days" },
]
