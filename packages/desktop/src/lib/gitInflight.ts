/**
 * In-flight promise dedupe for identical keys (e.g. concurrent git status
 * for the same session). Avoids double spawn when React Query + manual
 * invalidation race.
 */
const inflight = new Map<string, Promise<unknown>>()

export const withInflightDedupe = <T>(
  key: string,
  run: () => Promise<T>,
): Promise<T> => {
  const existing = inflight.get(key)
  if (existing) return existing as Promise<T>
  const promise = run().finally(() => {
    if (inflight.get(key) === promise) inflight.delete(key)
  })
  inflight.set(key, promise)
  return promise
}

export const __resetInflightForTests = (): void => {
  inflight.clear()
}
