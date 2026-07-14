import { Skeleton } from "../atoms"

/** Loading placeholder that mirrors sidebar section headers + session rows
 * (h-6 headers, h-7 single-line rows, one taller two-line row). */
export const SidebarSkeleton = () => {
  return (
    <div className="flex flex-col gap-2" role="status" aria-label="Loading sessions">
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-24 rounded-sm" />
        <Skeleton className="h-7 w-full rounded-sm" />
        <Skeleton className="h-10 w-full rounded-sm" />
        <Skeleton className="h-7 w-full rounded-sm" />
      </div>
      <div className="flex flex-col gap-px">
        <Skeleton className="mb-0.5 h-6 w-32 rounded-sm" />
        <Skeleton className="h-7 w-full rounded-sm" />
        <Skeleton className="h-10 w-full rounded-sm" />
      </div>
    </div>
  )
}
