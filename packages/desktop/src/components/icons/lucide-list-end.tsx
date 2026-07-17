import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `list-end` — local component per https://www.shadcn.io/icons */
export function ListEndIcon({
  size = 24,
  color = "currentColor",
  strokeWidth = 2,
  className,
  ...props
}: IconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn("shrink-0", className)}
      {...props}
      aria-hidden={props["aria-hidden"] ?? true}
    >
      <path d="M16 5H3" />
      <path d="M16 12H3" />
      <path d="M9 19H3" />
      <path d="m16 16-3 3 3 3" />
      <path d="M21 5v12a2 2 0 0 1-2 2h-6" />
    </svg>
  )
}
