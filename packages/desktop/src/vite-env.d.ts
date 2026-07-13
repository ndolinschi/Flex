/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** When `"true"` / `"1"`, show Automations UI. Default: off. */
  readonly VITE_AUTOMATIONS_UI?: string
  /** When `"true"` / `"1"`, show composer Flex mode. Default: off. */
  readonly VITE_FLEX_MODE?: string
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
