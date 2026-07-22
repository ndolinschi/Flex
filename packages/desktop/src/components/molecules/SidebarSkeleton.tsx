import { Skeleton } from "@/components/ui/skeleton"

/** Loading placeholder that mirrors sidebar section headers + session rows.
 * Row heights track `SessionListItem` (`min-h-7`, two-line ≈ h-10). Whisper
 * fills only — never bright shimmer slabs. */
export const SidebarSkeleton = () => {
  return (
    // px-2 matches row gutters (SessionListItem carries its own px — skeleton
    // is a block so it needs the gutter itself). rounded-sm mirrors session cells.
    <div className="flex flex-col gap-2 px-2" role="status" aria-label="Loading sessions">
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-24 rounded-sm" />
        <Skeleton className="h-7 min-h-7 w-full rounded-sm" />
        <Skeleton className="h-10 min-h-7 w-full rounded-sm" />
        <Skeleton className="h-7 min-h-7 w-full rounded-sm" />
      </div>
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-32 rounded-sm" />
        <Skeleton className="h-7 min-h-7 w-full rounded-sm" />
        <Skeleton className="h-10 min-h-7 w-full rounded-sm" />
      </div>
    </div>
  )
}
