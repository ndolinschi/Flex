import { ErrorBanner } from "./ErrorBanner"
import { FieldRow, SettingsSection } from "./SettingsSection"
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import type { SecretStorageMode } from "../../lib/types"

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
        <NativeSelect
          id="secretStorage"
          value={secretStorage ?? "file"}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value as SecretStorageMode)}
        >
          <NativeSelectOption value="file">
            Local file (no system prompts)
          </NativeSelectOption>
          {isMac ? (
            <NativeSelectOption value="keychain">
              System Keychain (OS-protected)
            </NativeSelectOption>
          ) : null}
        </NativeSelect>
      </FieldRow>
      {error ? (
        <div className="px-4 py-3">
          <ErrorBanner message={error} />
        </div>
      ) : null}
    </SettingsSection>
  )
}
