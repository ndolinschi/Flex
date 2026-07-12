/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** When `"true"` / `"1"`, show Automations UI. Default: off. */
  readonly VITE_AUTOMATIONS_UI?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
