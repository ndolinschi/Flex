import { Skeleton } from "@/components/ui/skeleton"

export const SidebarSkeleton = () => {
  return (
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
