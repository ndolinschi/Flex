export const isBrowserPreview = (): boolean =>
  typeof window !== "undefined" &&
  !(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__

export const NATIVE_APP_REQUIRED =
  "This feature needs the native desktop app (pnpm tauri dev). Browser preview has no backend."
