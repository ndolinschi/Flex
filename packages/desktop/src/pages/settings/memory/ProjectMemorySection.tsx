import { useQuery } from "@tanstack/react-query"
import {
  projectMemoryGet,
  projectMemoryList,
  projectMemoryRemove,
  projectMemorySetExpiry,
} from "../../../lib/tauri"
import { basename } from "../../../lib/utils"
import { EMPTY_MEMORIES, projectMemoryKey, type MemoryScope } from "./constants"
import { MemoryScopeSection } from "./MemoryScopeSection"

export const ProjectMemorySection = ({ cwd }: { cwd: string }) => {
  const queryKey = projectMemoryKey(cwd)
  const query = useQuery({
    queryKey,
    queryFn: () => projectMemoryList(cwd),
  })

  if (query.isLoading || query.isError) return null
  const memories = query.data ?? EMPTY_MEMORIES
  if (memories.length === 0) return null

  const scope: MemoryScope = {
    getMemory: (id) => projectMemoryGet(cwd, id),
    removeMemory: (id) => projectMemoryRemove(cwd, id),
    setExpiry: (id, expiresAtMs) => projectMemorySetExpiry(cwd, id, expiresAtMs),
    invalidateKey: queryKey,
  }

  const dimPrefix = cwd.replace(/\/[^/]+\/?$/, "/")

  return (
    <MemoryScopeSection
      title={basename(cwd)}
      hint={dimPrefix}
      memories={memories}
      scope={scope}
      defaultOpen={false}
    />
  )
}
