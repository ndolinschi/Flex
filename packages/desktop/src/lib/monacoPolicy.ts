/** Defaults for Monaco language services — syntax only (no project graph). */
export const MONACO_DEFAULT_DIAGNOSTICS = {
  noSemanticValidation: true,
  noSyntaxValidation: false,
} as const

export const shouldSubscribeMonacoMarkers = (
  modelPath: string | null,
  enabled: boolean,
): boolean => !!modelPath && enabled
