import { cn } from "../../lib/utils"

type AvatarProps = {
  label: string
  className?: string
}

export const Avatar = ({ label, className }: AvatarProps) => {
  const initial = label.trim().charAt(0).toUpperCase() || "?"

  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full",
        "bg-accent-subtle text-xs font-semibold text-accent",
        className,
      )}
    >
      {initial}
    </span>
  )
}
