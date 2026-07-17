import { Avatar as UiAvatar, AvatarFallback } from "@/components/ui/avatar"
import { cn } from "@/lib/utils"

type AvatarProps = {
  label: string
  className?: string
}

/** Initials avatar — shadcn Avatar + AvatarFallback. */
export const Avatar = ({ label, className }: AvatarProps) => {
  const initial = label.trim().charAt(0).toUpperCase() || "?"

  return (
    <UiAvatar
      size="sm"
      className={cn("size-7 after:hidden", className)}
      aria-hidden="true"
    >
      <AvatarFallback className="bg-accent-subtle text-xs font-semibold text-accent">
        {initial}
      </AvatarFallback>
    </UiAvatar>
  )
}
