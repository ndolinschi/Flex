import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  profileActivate,
  profileRemove,
  profileUpsert,
  profilesList,
  toInvokeError,
  validateProfile,
} from "../lib/tauri"
import type { ProviderProfileInput, ProviderProfileView } from "../lib/types"

const PROFILES_KEY = ["provider-profiles"] as const
const CONFIG_KEY = ["provider-config"] as const

/** CRUD + switching for named provider connections ("profiles"). Mirrors
 * `useProviderConfig`'s query/mutation shape. `validate` is form-values-only
 * (no persisted state involved) — it's the fix for Validate ignoring a
 * freshly typed key, so it must never read from the query cache. */
export const useProviderProfiles = () => {
  const queryClient = useQueryClient()

  const query = useQuery({
    queryKey: PROFILES_KEY,
    queryFn: profilesList,
    retry: 1,
  })

  const invalidateAfterChange = () => {
    void queryClient.invalidateQueries({ queryKey: PROFILES_KEY })
    void queryClient.invalidateQueries({ queryKey: CONFIG_KEY })
    void queryClient.invalidateQueries({ queryKey: ["models"] })
    void queryClient.invalidateQueries({ queryKey: ["sessions"] })
    void queryClient.invalidateQueries({ queryKey: ["builtin-providers"] })
  }

  const upsertMutation = useMutation({
    mutationFn: (input: ProviderProfileInput) => profileUpsert(input),
    onSuccess: invalidateAfterChange,
  })

  const removeMutation = useMutation({
    mutationFn: (id: string) => profileRemove(id),
    onSuccess: invalidateAfterChange,
  })

  const activateMutation = useMutation({
    mutationFn: (id: string) => profileActivate(id),
    onSuccess: invalidateAfterChange,
  })

  const validateMutation = useMutation({
    mutationFn: (input: ProviderProfileInput) => validateProfile(input),
  })

  const wrap = async <T,>(fn: () => Promise<T>): Promise<T> => {
    try {
      return await fn()
    } catch (err) {
      throw new Error(toInvokeError(err))
    }
  }

  return {
    profiles: query.data ?? ([] as ProviderProfileView[]),
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error ? toInvokeError(query.error) : null,
    upsert: (input: ProviderProfileInput) => wrap(() => upsertMutation.mutateAsync(input)),
    remove: (id: string) => wrap(() => removeMutation.mutateAsync(id)),
    activate: (id: string) => wrap(() => activateMutation.mutateAsync(id)),
    validate: (input: ProviderProfileInput) => wrap(() => validateMutation.mutateAsync(input)),
    isUpserting: upsertMutation.isPending,
    isRemoving: removeMutation.isPending,
    isActivating: activateMutation.isPending,
    isValidating: validateMutation.isPending,
  }
}
