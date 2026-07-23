import type { SessionId } from "./types"

export const GROUP_PALETTE: readonly string[] = [
  "#5E9BF0",
  "#63C07A",
  "#E07B5F",
  "#C47ED4",
  "#E0B84A",
  "#5BBFCC",
  "#E080A8",
  "#7BA3E0",
]

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

const djb2 = (s: string): number => {
  let h = 5381
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) + h + s.charCodeAt(i)) >>> 0
  }
  return h >>> 1
}

export const sessionColor = (sessionId: SessionId): string =>
  SESSION_PALETTE[djb2(sessionId) % SESSION_PALETTE.length]!
