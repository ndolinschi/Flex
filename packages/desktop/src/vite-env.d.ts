/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** When `"true"` / `"1"`, show Automations UI. Default: off. */
  readonly VITE_AUTOMATIONS_UI?: string
  /** When `"true"` / `"1"`, show composer Flex mode. Default: off. */
  readonly VITE_FLEX_MODE?: string
  /** When `"true"` / `"1"`, show right-panel Memory tab. Default: off. */
  readonly VITE_MEMORY_TAB?: string
  /** When `"false"` / `"0"`, hide Database UI plugin tab. Default: on. */
  readonly VITE_DATABASE_TAB?: string
  /** When `"true"` / `"1"`, show Components UI plugin tab. Default: off. */
  readonly VITE_COMPONENTS_TAB?: string
  /** When `"false"` / `"0"`, disable inline prompt completion plugin. Default: on. */
  readonly VITE_INLINE_COMPLETION?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

/** Monaco worker factory — set by `lib/monacoEnv.ts` before editor mount. */
interface MonacoEnvironment {
  getWorker(workerId: string, label: string): Worker
}

interface Window {
  MonacoEnvironment?: MonacoEnvironment
}

declare var MonacoEnvironment: MonacoEnvironment | undefined
