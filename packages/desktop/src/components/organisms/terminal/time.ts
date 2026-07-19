import { useEffect, useState } from "react"

export const formatElapsed = (createdAtMs: number, nowMs: number): string => {
  const diff = nowMs - createdAtMs
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return "now"
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  return `${hours}h`
}

export const useNowTicker = (
  intervalMs: number,
  enabled = true,
): number => {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!enabled) return
    const id = setInterval(() => setNow(Date.now()), intervalMs)
    return () => clearInterval(id)
  }, [intervalMs, enabled])
  return now
}
