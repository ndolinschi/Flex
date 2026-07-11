import { isBrowserPreview } from "./browserMock"

/**
 * Native "turn finished" notification (the reference design parity). Dynamically imports
 * the notification plugin so a browser-preview build never touches Tauri
 * internals, requests permission lazily on first use, and swallows every
 * error — a broken notification must never take down the app.
 */
export const notifyTurnCompleted = async (
  sessionTitle: string,
  ok: boolean,
): Promise<void> => {
  if (isBrowserPreview()) return

  try {
    const { isPermissionGranted, requestPermission, sendNotification } =
      await import("@tauri-apps/plugin-notification")

    let granted = await isPermissionGranted()
    if (!granted) {
      const permission = await requestPermission()
      granted = permission === "granted"
    }
    if (!granted) return

    sendNotification({
      title: ok ? "Agent finished" : "Agent error",
      body: sessionTitle,
    })
  } catch {
    // Notifications are best-effort only — never surface this to the user.
  }
}
