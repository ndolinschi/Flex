import { isBrowserPreview } from "./browserPreview"

/**
 * Native "turn finished" notification (the reference design parity). Dynamically imports
 * the notification plugin so a browser-preview build never touches Tauri
 * internals, requests permission lazily on first use, and swallows every
 * error — a broken notification must never take down the app.
 *
 * Permission is checked-and-requested lazily on the FIRST actual send (not at
 * app boot) and cached for the lifetime of the page — macOS silently no-ops a
 * `sendNotification` call when permission was never granted, which was the
 * root cause of "notifications don't work": nothing upstream of this file
 * was gating on `isPermissionGranted()`/`requestPermission()` at all, so a
 * user who never saw (or dismissed) the OS permission prompt got silent
 * no-ops forever. Once denied, we cache that too and skip quietly — no
 * repeated prompts.
 */
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
    // No Tauri notification plugin in browser preview — degrade to a no-op
    // (the Web Notification API needs its own permission prompt too, and
    // preview is for layout/behavior verification, not native notification
    // testing). Logged so a headless preview check can assert this path fired.
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
    // Notifications are best-effort only — never surface this to the user.
  }
}

/** Shared AudioContext for the completion chime — created lazily on first
 * play (browsers require a user gesture before audio can start; by the time
 * a turn completes the user has almost always interacted with the page
 * already, e.g. sending the prompt that triggered the turn). */
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

/**
 * Short, subtle two-note completion chime — synthesized with WebAudio
 * oscillators instead of shipping an embedded sound asset (simplest
 * dependency-free option: no base64 file to maintain). ~200ms total,
 * two notes a fifth apart with a quick exponential decay so it reads as a
 * gentle "done" blip rather than an alert.
 */
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

      // Quick attack, exponential decay — keeps it subtle rather than a beep.
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
    // Sound is best-effort only — never surface this to the user.
  }
}
