import type { RoutineDto, RoutineRunRecordDto } from "../../../lib/types"

export const ROUTINES_KEY = ["routines"] as const

export const EMPTY_HISTORY: RoutineRunRecordDto[] = []
export const EMPTY_ROUTINES: RoutineDto[] = []

export const KEBAB_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/
