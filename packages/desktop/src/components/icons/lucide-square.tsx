import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `square` — local component per https://www.shadcn.io/icons */
export function SquareIcon({
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
      <rect width="18" height="18" x="3" y="3" rx="2" />
    </svg>
  )
}
