import {
  Avatar as AvatarPrimitive,
  AvatarFallback,
} from "@/components/ui/avatar"
import { cn } from "@/lib/utils"

type AvatarProps = {
  label: string
  className?: string
}

export const Avatar = ({ label, className }: AvatarProps) => {
  const initial = label.trim().charAt(0).toUpperCase() || "?"
  return (
    <AvatarPrimitive
      aria-hidden="true"
      className={cn("size-7", className)}
    >
      <AvatarFallback className="bg-accent-subtle text-xs font-semibold text-accent">
        {initial}
      </AvatarFallback>
    </AvatarPrimitive>
  )
}
