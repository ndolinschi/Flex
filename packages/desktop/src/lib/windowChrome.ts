import { getCurrentWindow } from "@tauri-apps/api/window"

/** Host OS for window-chrome layout (traffic lights vs Windows controls). */
export type WindowHost = "macos" | "windows" | "linux" | "unknown"

export const detectWindowHost = (): WindowHost => {
  if (typeof navigator === "undefined") return "unknown"
  const platform = navigator.platform ?? ""
  const ua = navigator.userAgent ?? ""
  if (/Mac|iPhone|iPad|iPod/i.test(platform) || /Mac OS X/i.test(ua)) {
    return "macos"
  }
  if (/Win/i.test(platform) || /Windows/i.test(ua)) return "windows"
  if (/Linux/i.test(platform) || /Linux/i.test(ua)) return "linux"
  return "unknown"
}

export const appWindow = () => getCurrentWindow()

export const minimizeWindow = async (): Promise<void> => {
  await appWindow().minimize()
}

export const toggleMaximizeWindow = async (): Promise<void> => {
  await appWindow().toggleMaximize()
}

/** Toggle native fullscreen (macOS spaces). Prefer this over maximize on Mac. */
export const toggleFullscreenWindow = async (): Promise<void> => {
  const win = appWindow()
  const fullscreen = await win.isFullscreen()
  await win.setFullscreen(!fullscreen)
}

/**
 * Platform zoom affordance: macOS green / title-bar double-click → fullscreen;
 * Windows/Linux caption maximize → toggleMaximize.
 */
export const toggleZoomWindow = async (): Promise<void> => {
  if (detectWindowHost() === "macos") {
    await toggleFullscreenWindow()
    return
  }
  await toggleMaximizeWindow()
}

export const closeWindow = async (): Promise<void> => {
  await appWindow().close()
}

export const isWindowMaximized = async (): Promise<boolean> => {
  return appWindow().isMaximized()
}

export const isWindowFullscreen = async (): Promise<boolean> => {
  return appWindow().isFullscreen()
}
