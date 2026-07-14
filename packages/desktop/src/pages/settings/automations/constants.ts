import { cn } from "../../../lib/utils"
import type { RoutineDto, RoutineRunRecordDto } from "../../../lib/types"

export const ROUTINES_KEY = ["routines"] as const

export const EMPTY_HISTORY: RoutineRunRecordDto[] = []
export const EMPTY_ROUTINES: RoutineDto[] = []

export const selectClasses = cn(
  "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink",
  "focus:border-stroke-2 focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-stroke-2)]",
)

export const KEBAB_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/
