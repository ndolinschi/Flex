import { clampSplitRatio } from "../../../stores/contentLayoutModel"

export const CONTENT_LEFT_PANEL_ID = "content-left"
export const CONTENT_RIGHT_PANEL_ID = "content-right"

/** Pure layout map for the content split (safe to unit-test without React tree). */
export const contentWorkspaceDefaultLayout = (
  split: boolean,
  splitRatio: number,
): Record<string, number> => {
  if (!split) return { [CONTENT_LEFT_PANEL_ID]: 100 }
  const left = Math.round(clampSplitRatio(splitRatio) * 100)
  return {
    [CONTENT_LEFT_PANEL_ID]: left,
    [CONTENT_RIGHT_PANEL_ID]: 100 - left,
  }
}
