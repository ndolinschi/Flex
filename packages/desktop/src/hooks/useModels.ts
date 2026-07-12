import { useQuery } from "@tanstack/react-query"
import { listBuiltinProviders, listModels, toInvokeError } from "../lib/tauri"

const MODELS_KEY = ["models"] as const
const BUILTIN_KEY = ["builtin-providers"] as const

export const useModels = (enabled = true) => {
  const modelsQuery = useQuery({
    queryKey: MODELS_KEY,
    queryFn: listModels,
    enabled,
    retry: 1,
    staleTime: 60_000,
  })

  const builtinQuery = useQuery({
    queryKey: BUILTIN_KEY,
    queryFn: listBuiltinProviders,
    staleTime: Infinity,
  })

  return {
    models: modelsQuery.data ?? [],
    builtinProviders: builtinQuery.data ?? [],
    isLoading: modelsQuery.isLoading || builtinQuery.isLoading,
    isError: modelsQuery.isError,
    error: modelsQuery.error ? toInvokeError(modelsQuery.error) : null,
    refetchModels: modelsQuery.refetch,
  }
}
