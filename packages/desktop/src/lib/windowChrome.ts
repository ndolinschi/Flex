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

export const closeWindow = async (): Promise<void> => {
  await appWindow().close()
}

export const isWindowMaximized = async (): Promise<boolean> => {
  return appWindow().isMaximized()
}
