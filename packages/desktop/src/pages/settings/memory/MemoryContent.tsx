import { useQuery } from "@tanstack/react-query"
import { FileText } from "lucide-react"
import { Spinner } from "../../../components/atoms"
import { EmptyState, ErrorBanner } from "../../../components/molecules"
import {
  memoryGet,
  memoryList,
  memoryRemove,
  memorySetExpiry,
  toInvokeError,
} from "../../../lib/tauri"
import { EMPTY_MEMORIES, MEMORY_KEY, type MemoryScope } from "./constants"
import { MemoryRow } from "./MemoryRow"
import { ProjectMemorySection } from "./ProjectMemorySection"
import { useProjectCwds } from "./useProjectCwds"

export const MemoryContent = () => {
  const memoryQuery = useQuery({
    queryKey: MEMORY_KEY,
    queryFn: memoryList,
  })
  const projectCwds = useProjectCwds()

  const memories = memoryQuery.data ?? EMPTY_MEMORIES

  const globalScope: MemoryScope = {
    getMemory: memoryGet,
    removeMemory: memoryRemove,
    setExpiry: memorySetExpiry,
    invalidateKey: MEMORY_KEY,
  }

  const isEmpty = !memoryQuery.isLoading && !memoryQuery.isError && memories.length === 0

  return (
    <div className="flex flex-col gap-3">
      <section data-settings-row="memory-global">
        <div className="mb-2 flex items-center gap-2 px-3.5">
          <h2 className="text-sm leading-4 text-ink-secondary">Global</h2>
          <span className="text-xs text-ink-faint">{memories.length}</span>
        </div>
        {memoryQuery.isLoading ? (
          <div className="flex items-center justify-center gap-2 px-4 py-8 text-xs text-ink-muted">
            <Spinner size="sm" /> Loading memory…
          </div>
        ) : memoryQuery.isError ? (
          <ErrorBanner message={toInvokeError(memoryQuery.error)} />
        ) : isEmpty ? (
          <EmptyState
            icon={<FileText className="h-5 w-5" aria-hidden />}
            title="No memories yet"
            description="The agent saves reusable knowledge here as it works."
          />
        ) : (
          <div className="flex flex-col gap-1.5">
            {memories.map((memory) => (
              <MemoryRow key={memory.id} memory={memory} scope={globalScope} />
            ))}
          </div>
        )}
      </section>

      {projectCwds.map((cwd) => (
        <ProjectMemorySection key={cwd} cwd={cwd} />
      ))}
    </div>
  )
}
