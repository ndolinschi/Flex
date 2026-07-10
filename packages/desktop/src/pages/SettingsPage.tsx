import { SettingsShell } from "../components/templates"
import { ProviderSettingsForm } from "../components/organisms"

type SettingsPageProps = {
  embedded?: boolean
}

export const SettingsPage = ({ embedded = false }: SettingsPageProps) => {
  return (
    <SettingsShell embedded={embedded}>
      <p className="mb-4 text-sm text-ink-muted">
        Configure the preferred native provider for the agent loop.
      </p>
      <ProviderSettingsForm />
    </SettingsShell>
  )
}
