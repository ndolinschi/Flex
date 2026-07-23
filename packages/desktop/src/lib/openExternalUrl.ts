export const openExternalUrl = async (url: string): Promise<void> => {
  const { openUrl } = await import("@tauri-apps/plugin-opener")
  await openUrl(url)
}
