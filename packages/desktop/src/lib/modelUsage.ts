import type { TokenUsage } from "./types"

export type ModelUsageBucket = {
  input: number
  output: number
  cacheRead: number
  cacheWrite: number
  calls: number
}

export type ModelUsageMap = Record<string, ModelUsageBucket>

export const emptyModelUsageBucket = (): ModelUsageBucket => ({
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  calls: 0,
})

export const addUsageToModelMap = (
  map: ModelUsageMap,
  model: string,
  usage: TokenUsage,
  calls = 1,
): ModelUsageMap => {
  const key = model.trim()
  if (!key) return map
  const prev = map[key] ?? emptyModelUsageBucket()
  return {
    ...map,
    [key]: {
      input: prev.input + usage.input,
      output: prev.output + usage.output,
      cacheRead: prev.cacheRead + (usage.cache_read ?? 0),
      cacheWrite: prev.cacheWrite + (usage.cache_write ?? 0),
      calls: prev.calls + calls,
    },
  }
}

export const cacheTotalsFromModelUsage = (
  map: ModelUsageMap | undefined,
): { cacheRead: number; cacheWrite: number } => {
  if (!map) return { cacheRead: 0, cacheWrite: 0 }
  let cacheRead = 0
  let cacheWrite = 0
  for (const b of Object.values(map)) {
    cacheRead += b.cacheRead
    cacheWrite += b.cacheWrite
  }
  return { cacheRead, cacheWrite }
}
