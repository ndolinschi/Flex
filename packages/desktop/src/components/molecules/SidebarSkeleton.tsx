import { Skeleton } from "@/components/ui/skeleton"

/** Loading placeholder that mirrors sidebar section headers + session rows.
 * Row heights track production agent list (`h-8`) + section labels. Whisper
 * fills only — never bright shimmer slabs. */
export const SidebarSkeleton = () => {
  return (
    // No horizontal pad — cells own their 8px gutter (glass agent-sidebar-list).
    <div className="flex flex-col gap-px pb-2" role="status" aria-label="Loading sessions">
      <div className="flex flex-col gap-px">
        <Skeleton className="my-0.5 h-8 w-20 rounded-sm" />
        <Skeleton className="h-8 w-full rounded-sm" />
        <Skeleton className="h-8 w-full rounded-sm" />
        <Skeleton className="h-8 w-full rounded-sm" />
      </div>
      <div className="flex flex-col gap-px">
        <Skeleton className="my-0.5 h-8 w-28 rounded-sm" />
        <Skeleton className="h-8 w-full rounded-sm" />
        <Skeleton className="h-8 w-full rounded-sm" />
      </div>
    </div>
  )
}
