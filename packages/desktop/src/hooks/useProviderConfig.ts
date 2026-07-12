import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  getProviderConfig,
  saveProviderConfig,
  setSecretStorage,
  toInvokeError,
  validateProvider,
} from "../lib/tauri"
import type {
  ProviderConfigView,
  SaveProviderConfigInput,
  SecretStorageMode,
} from "../lib/types"

const CONFIG_KEY = ["provider-config"] as const

export const useProviderConfig = () => {
  const queryClient = useQueryClient()

  const query = useQuery({
    queryKey: CONFIG_KEY,
    queryFn: getProviderConfig,
    retry: 1,
  })

  const saveMutation = useMutation({
    mutationFn: (input: SaveProviderConfigInput) => saveProviderConfig(input),
    onSuccess: (view: ProviderConfigView) => {
      queryClient.setQueryData(CONFIG_KEY, view)
      void queryClient.invalidateQueries({ queryKey: ["models"] })
      void queryClient.invalidateQueries({ queryKey: ["sessions"] })
      void queryClient.invalidateQueries({ queryKey: ["builtin-providers"] })
    },
  })

  const validateMutation = useMutation({
    mutationFn: (input: SaveProviderConfigInput) => validateProvider(input),
  })

  const secretStorageMutation = useMutation({
    mutationFn: (mode: SecretStorageMode) => setSecretStorage(mode),
    onSuccess: (view: ProviderConfigView) => {
      queryClient.setQueryData(CONFIG_KEY, view)
    },
  })

  const handleSave = async (input: SaveProviderConfigInput) => {
    try {
      return await saveMutation.mutateAsync(input)
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  const handleValidate = async (input: SaveProviderConfigInput) => {
    try {
      return await validateMutation.mutateAsync(input)
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  const handleSetSecretStorage = async (mode: SecretStorageMode) => {
    try {
      return await secretStorageMutation.mutateAsync(mode)
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  return {
    config: query.data,
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error ? toInvokeError(query.error) : null,
    save: handleSave,
    validate: handleValidate,
    setSecretStorage: handleSetSecretStorage,
    isSaving: saveMutation.isPending,
    isValidating: validateMutation.isPending,
    isSettingSecretStorage: secretStorageMutation.isPending,
    saveError: saveMutation.error ? toInvokeError(saveMutation.error) : null,
    validateError: validateMutation.error
      ? toInvokeError(validateMutation.error)
      : null,
    secretStorageError: secretStorageMutation.error
      ? toInvokeError(secretStorageMutation.error)
      : null,
  }
}
