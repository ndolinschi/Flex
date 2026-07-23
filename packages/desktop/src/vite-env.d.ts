/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_AUTOMATIONS_UI?: string
  readonly VITE_FLEX_MODE?: string
  readonly VITE_MEMORY_TAB?: string
  readonly VITE_DATABASE_TAB?: string
  readonly VITE_COMPONENTS_TAB?: string
  readonly VITE_INLINE_COMPLETION?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

interface MonacoEnvironment {
  getWorker(workerId: string, label: string): Worker
}

interface Window {
  MonacoEnvironment?: MonacoEnvironment
}

declare var MonacoEnvironment: MonacoEnvironment | undefined
