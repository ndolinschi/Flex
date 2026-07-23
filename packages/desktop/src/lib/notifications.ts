import { isBrowserPreview } from "./browserPreview"

let permissionCache: "granted" | "denied" | null = null

const ensurePermission = async (): Promise<boolean> => {
  if (permissionCache === "granted") return true
  if (permissionCache === "denied") return false

  const { isPermissionGranted, requestPermission } = await import(
    "@tauri-apps/plugin-notification"
  )

  let granted = await isPermissionGranted()
  if (!granted) {
    const permission = await requestPermission()
    granted = permission === "granted"
  }
  permissionCache = granted ? "granted" : "denied"
  return granted
}

export const notifyTurnCompleted = async (
  sessionTitle: string,
  ok: boolean,
): Promise<void> => {
  if (isBrowserPreview()) {
    console.debug("[notifications] browser preview — skipping native notification", {
      sessionTitle,
      ok,
    })
    return
  }

  try {
    const granted = await ensurePermission()
    if (!granted) return

    const { sendNotification } = await import("@tauri-apps/plugin-notification")
    sendNotification({
      title: ok ? "Agent finished" : "Agent error",
      body: sessionTitle,
    })
  } catch {
  }
}

let chimeAudioContext: AudioContext | null = null

const getAudioContext = (): AudioContext | null => {
  if (typeof window === "undefined") return null
  const Ctor =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext?: typeof AudioContext })
      .webkitAudioContext
  if (!Ctor) return null
  if (!chimeAudioContext) {
    chimeAudioContext = new Ctor()
  }
  return chimeAudioContext
}

export const playCompletionChime = (): void => {
  try {
    const ctx = getAudioContext()
    if (!ctx) return
    if (ctx.state === "suspended") {
      void ctx.resume()
    }

    const now = ctx.currentTime
    const notes: Array<{ freq: number; start: number; duration: number }> = [
      { freq: 660, start: 0, duration: 0.11 },
      { freq: 880, start: 0.09, duration: 0.14 },
    ]

    for (const note of notes) {
      const oscillator = ctx.createOscillator()
      const gain = ctx.createGain()
      oscillator.type = "sine"
      oscillator.frequency.setValueAtTime(note.freq, now + note.start)

      const peak = 0.08
      gain.gain.setValueAtTime(0.0001, now + note.start)
      gain.gain.exponentialRampToValueAtTime(peak, now + note.start + 0.012)
      gain.gain.exponentialRampToValueAtTime(
        0.0001,
        now + note.start + note.duration,
      )

      oscillator.connect(gain)
      gain.connect(ctx.destination)
      oscillator.start(now + note.start)
      oscillator.stop(now + note.start + note.duration + 0.02)
    }
  } catch {
  }
}
