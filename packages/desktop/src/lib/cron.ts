
const WEEKDAYS: Record<string, string> = {
  "0": "Sundays",
  "1": "Mondays",
  "2": "Tuesdays",
  "3": "Wednesdays",
  "4": "Thursdays",
  "5": "Fridays",
  "6": "Saturdays",
  "7": "Sundays",
}

const pad2 = (n: string): string => n.padStart(2, "0")

export const humanizeCron = (expr: string): string => {
  const trimmed = expr.trim()
  const parts = trimmed.split(/\s+/)
  if (parts.length !== 5) return trimmed

  const [minute, hour, dom, month, dow] = parts
  const atTime = `${pad2(hour)}:${pad2(minute)}`

  if (dom === "*" && month === "*") {
    if (minute === "*" && hour === "*" && dow === "*") {
      return "Every minute"
    }

    const everyNMinutes = minute.match(/^\*\/(\d+)$/)
    if (everyNMinutes && hour === "*" && dow === "*") {
      return `Every ${everyNMinutes[1]} minutes`
    }

    if (/^\d+$/.test(minute) && hour === "*" && dow === "*") {
      return "Hourly"
    }

    if (/^\d+$/.test(minute) && /^\d+$/.test(hour)) {
      if (dow === "*") {
        return `Daily at ${atTime}`
      }
      if (dow === "1-5") {
        return `Weekdays at ${atTime}`
      }
      if (dow === "0,6" || dow === "6,0") {
        return `Weekends at ${atTime}`
      }
      const weekday = WEEKDAYS[dow]
      if (weekday) {
        return `${weekday} at ${atTime}`
      }
    }
  }

  return trimmed
}
