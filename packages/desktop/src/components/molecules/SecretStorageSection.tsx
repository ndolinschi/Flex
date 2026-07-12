import { ErrorBanner } from "./ErrorBanner"
import { FieldRow, SettingsSection } from "./SettingsSection"
import type { SecretStorageMode } from "../../lib/types"

const selectClassName =
  "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"

type SecretStorageSectionProps = {
  secretStorage: SecretStorageMode | undefined
  isMac: boolean
  disabled: boolean
  error: string | null
  onChange: (mode: SecretStorageMode) => void
}

/** Security section — where the encryption key for stored API keys lives. */
export const SecretStorageSection = ({
  secretStorage,
  isMac,
  disabled,
  error,
  onChange,
}: SecretStorageSectionProps) => {
  return (
    <SettingsSection
      title="Security"
      description="Where the encryption key for your stored API keys lives"
      rowId="behavior-secret-storage"
      className="mb-0"
    >
      <FieldRow
        label="Secret storage"
        htmlFor="secretStorage"
        hint={
          secretStorage === "keychain"
            ? "System Keychain is OS-protected, but macOS may prompt for access — especially on dev builds that re-sign on every rebuild."
            : isMac
              ? "Local file stores the encryption key on disk, readable by your user account — no system prompts, ever. System Keychain is OS-protected but may prompt."
              : "Local file stores the encryption key on disk, readable by your user account — no system prompts, ever. System Keychain is only available on macOS."
        }
      >
        <select
          id="secretStorage"
          value={secretStorage ?? "file"}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value as SecretStorageMode)}
          className={selectClassName}
        >
          <option value="file">Local file (no system prompts)</option>
          {isMac ? (
            <option value="keychain">System Keychain (OS-protected)</option>
          ) : null}
        </select>
      </FieldRow>
      {error ? (
        <div className="px-4 py-3">
          <ErrorBanner message={error} />
        </div>
      ) : null}
    </SettingsSection>
  )
}
