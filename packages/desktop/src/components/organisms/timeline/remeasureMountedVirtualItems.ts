/**
 * Minimal surface needed to remeasure mounted rows — avoids coupling to
 * Virtualizer's scroll-element generics (`HTMLDivElement` vs `Element`).
 */
export type RemeasurableVirtualizer = {
  getVirtualItems: () => ReadonlyArray<{ key: string | number | bigint; index: number }>
  elementsCache: Map<string | number | bigint, unknown>
  resizeItem: (index: number, size: number) => void
}

const isMeasurableElement = (
  node: unknown,
): node is { offsetHeight: number } =>
  typeof node === "object" &&
  node !== null &&
  typeof (node as { offsetHeight?: unknown }).offsetHeight === "number"

/**
 * Remeasure currently mounted virtual rows in place — read `offsetHeight`
 * and push into `resizeItem` without clearing `itemSizeCache`.
 *
 * Prefer this over `virtualizer.measure()`, which wipes the entire size
 * cache and falls back to `estimateSize`. Absolute `translateY` rows then
 * stack on top of each other until a ResizeObserver re-fires — and RO often
 * does **not** re-fire when the DOM height already matches the last
 * observation (common after streaming growth and after scroll remounts on
 * WebView2).
 *
 * Always write the measured size — including `0` — so collapsed /
 * null-rendered rows can shrink an inflated cache (overestimates leave
 * persistent gaps; underestimates grow via RO on the next paint).
 */
export const remeasureMountedVirtualItems = (
  virtualizer: RemeasurableVirtualizer,
): void => {
  for (const item of virtualizer.getVirtualItems()) {
    const el = virtualizer.elementsCache.get(item.key)
    if (!isMeasurableElement(el)) continue
    virtualizer.resizeItem(item.index, el.offsetHeight)
  }
}
