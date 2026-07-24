/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_AUTOMATIONS_UI?: string
  readonly VITE_FLEX_MODE?: string
  readonly VITE_MEMORY_TAB?: string
  readonly VITE_DATABASE_TAB?: string
  readonly VITE_COMPONENTS_TAB?: string
  readonly VITE_ARTIFACTS_TAB?: string
  readonly VITE_STATUS_TAB?: string
  readonly VITE_PROMPT_TAB?: string
  readonly VITE_CHANGES_TAB?: string
  readonly VITE_PR_TAB?: string
  readonly VITE_TERMINAL_TAB?: string
  readonly VITE_BROWSER_TAB?: string
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
