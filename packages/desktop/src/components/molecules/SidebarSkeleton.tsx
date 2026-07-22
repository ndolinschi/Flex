import { Skeleton } from "@/components/ui/skeleton"

/** Loading placeholder that mirrors sidebar section headers + session rows.
 * Row heights track `SessionListItem` (`min-h-7`, two-line ≈ h-10). Whisper
 * fills only — never bright shimmer slabs. */
export const SidebarSkeleton = () => {
  return (
    <div className="flex flex-col gap-2" role="status" aria-label="Loading sessions">
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-24 rounded-md" />
        <Skeleton className="min-h-7 h-7 w-full rounded-md" />
        <Skeleton className="min-h-7 h-10 w-full rounded-md" />
        <Skeleton className="min-h-7 h-7 w-full rounded-md" />
      </div>
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-32 rounded-md" />
        <Skeleton className="min-h-7 h-7 w-full rounded-md" />
        <Skeleton className="min-h-7 h-10 w-full rounded-md" />
      </div>
    </div>
  )
}
