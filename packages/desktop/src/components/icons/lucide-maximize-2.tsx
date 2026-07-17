import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `maximize-2` — local component per https://www.shadcn.io/icons */
export function Maximize2Icon({
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
      <path d="M15 3h6v6" />
      <path d="m21 3-7 7" />
      <path d="m3 21 7-7" />
      <path d="M9 21H3v-6" />
    </svg>
  )
}
