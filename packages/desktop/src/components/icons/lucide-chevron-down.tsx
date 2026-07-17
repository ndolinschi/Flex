import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `chevron-down` — local component per https://www.shadcn.io/icons */
export function ChevronDownIcon({
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
      <path d="m6 9 6 6 6-6" />
    </svg>
  )
}
