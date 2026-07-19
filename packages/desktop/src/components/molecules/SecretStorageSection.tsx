import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ErrorBanner } from "./ErrorBanner"
import { FieldRow, SettingsSection } from "./SettingsSection"
import type { SecretStorageMode } from "../../lib/types"

const SECRET_STORAGE_ITEMS: Array<{ value: SecretStorageMode; label: string }> =
  [
    { value: "file", label: "Local file (no system prompts)" },
    { value: "keychain", label: "System Keychain (OS-protected)" },
  ]

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
  const items = isMac
    ? SECRET_STORAGE_ITEMS
    : SECRET_STORAGE_ITEMS.filter((item) => item.value !== "keychain")

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
        <Select
          items={items}
          value={secretStorage ?? "file"}
          disabled={disabled}
          onValueChange={(v) => {
            if (v == null) return
            onChange(v as SecretStorageMode)
          }}
        >
          <SelectTrigger id="secretStorage" className="w-full" size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {items.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </FieldRow>
      {error ? (
        <div className="px-4 py-3">
          <ErrorBanner message={error} />
        </div>
      ) : null}
    </SettingsSection>
  )
}
