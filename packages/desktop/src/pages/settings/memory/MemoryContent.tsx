import { useQuery } from "@tanstack/react-query"
import { FileText } from "lucide-react"
import { Spinner } from "../../../components/atoms"
import {
  EmptyState,
  ErrorBanner,
  ToolQueryError,
} from "../../../components/molecules"
import {
  memoryGet,
  memoryList,
  memoryRemove,
  memorySetExpiry,
  toInvokeError,
} from "../../../lib/tauri"
import { cn } from "../../../lib/utils"
import { EMPTY_MEMORIES, MEMORY_KEY, type MemoryScope } from "./constants"
import { MemoryRow } from "./MemoryRow"
import { ProjectMemorySection } from "./ProjectMemorySection"
import { useProjectCwds } from "./useProjectCwds"

type MemoryContentProps = {
  /** Tool-tab density: tighter section pads and ToolQueryError for load fail. */
  compact?: boolean
}

export const MemoryContent = ({ compact = false }: MemoryContentProps) => {
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

  const isEmpty =
    !memoryQuery.isLoading && !memoryQuery.isError && memories.length === 0

  const sectionPad = compact ? "px-0" : "px-3.5"
  const stackGap = compact ? "gap-2" : "gap-3"

  return (
    <div className={cn("flex flex-col", stackGap)}>
      <section data-settings-row="memory-global">
        <div className={cn("mb-2 flex items-center gap-2", sectionPad)}>
          <h2 className="text-sm leading-4 text-ink-secondary">Global</h2>
          <span className="text-xs text-ink-faint">{memories.length}</span>
        </div>
        {memoryQuery.isLoading ? (
          <div
            className={cn(
              "flex items-center justify-center gap-2 py-8 text-xs text-ink-muted",
              compact ? "px-2.5" : "px-4",
            )}
          >
            <Spinner size="sm" /> Loading memory…
          </div>
        ) : memoryQuery.isError ? (
          compact ? (
            <ToolQueryError
              title="Couldn't load memory"
              error={memoryQuery.error}
              fallbackMessage="Failed to load global memory notes."
              onRetry={() => void memoryQuery.refetch()}
              retrying={memoryQuery.isFetching}
              className="py-8"
            />
          ) : (
            <ErrorBanner message={toInvokeError(memoryQuery.error)} />
          )
        ) : isEmpty ? (
          <EmptyState
            icon={<FileText className="h-5 w-5" aria-hidden />}
            title="No memories yet"
            description="The agent saves reusable knowledge here as it works."
            className={compact ? "py-8" : undefined}
          />
        ) : (
          <div className={cn("flex flex-col gap-1.5", compact && "gap-1")}>
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
