import type { IconProps } from "./types"
import { cn } from "@/lib/utils"

/** Lucide `list-todo` — local component per https://www.shadcn.io/icons */
export function ListTodoIcon({
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
      <path d="M13 5h8" />
      <path d="M13 12h8" />
      <path d="M13 19h8" />
      <path d="m3 17 2 2 4-4" />
      <rect x="3" y="4" width="6" height="6" rx="1" />
    </svg>
  )
}
