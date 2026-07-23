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

export const remeasureMountedVirtualItems = (
  virtualizer: RemeasurableVirtualizer,
): void => {
  for (const item of virtualizer.getVirtualItems()) {
    const el = virtualizer.elementsCache.get(item.key)
    if (!isMeasurableElement(el)) continue
    virtualizer.resizeItem(item.index, el.offsetHeight)
  }
}
