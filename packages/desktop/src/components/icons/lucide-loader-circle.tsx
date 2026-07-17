import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `loader-circle` — local component per https://www.shadcn.io/icons */
export function Loader2Icon({
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
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  )
}
