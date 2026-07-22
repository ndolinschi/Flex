import type { SessionId } from "./types"

/**
 * Quiet 6-color palette for tab groups and session-affinity dots.
 * Chosen for visibility on both dark and light backgrounds while staying
 * perceptually distinct without being loud.
 */
export const GROUP_PALETTE: readonly string[] = [
  "#5E9BF0", // blue
  "#63C07A", // green
  "#E07B5F", // coral
  "#C47ED4", // purple
  "#E0B84A", // amber
  "#5BBFCC", // teal
  "#E080A8", // pink
  "#7BA3E0", // periwinkle
]

/**
 * Stable palette for session-affinity dots (12 distinct hues).
 * Each sessionId gets a deterministic index via djb2 hash so the color is
 * stable across renders and restarts.
 */
const SESSION_PALETTE: readonly string[] = [
  "#5E9BF0",
  "#63C07A",
  "#E07B5F",
  "#C47ED4",
  "#E0B84A",
  "#5BBFCC",
  "#E080A8",
  "#7BA3E0",
  "#D4894A",
  "#60C4B0",
  "#9A78E0",
  "#C0C060",
]

/**
 * Fast djb2 hash of a string — deterministic across V8 restarts.
 * Returns a non-negative 31-bit integer.
 */
const djb2 = (s: string): number => {
  let h = 5381
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) + h + s.charCodeAt(i)) >>> 0
  }
  return h >>> 1
}

/** Stable CSS color string for a session's affinity dot. */
export const sessionColor = (sessionId: SessionId): string =>
  SESSION_PALETTE[djb2(sessionId) % SESSION_PALETTE.length]!
