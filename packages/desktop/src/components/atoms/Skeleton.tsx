import { Skeleton as UiSkeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

type SkeletonProps = {
  className?: string
}

export const Skeleton = ({ className }: SkeletonProps) => {
  return (
    <UiSkeleton
      aria-hidden="true"
      className={cn("bg-surface-muted", className)}
    />
  )
}
