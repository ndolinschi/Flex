import { Kbd } from "../components/atoms"
import { ProviderSettingsForm } from "../components/organisms"

export const WelcomePage = () => {
  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="mx-auto flex w-full max-w-[var(--welcome-rail)] flex-1 flex-col justify-center px-4 py-8">
        <p className="mb-1.5 text-xs font-medium uppercase tracking-widest text-ink-faint">
          Agent Desktop
        </p>
        <h1 className="mb-2 text-xl font-semibold text-ink">
          Configure a provider to get started
        </h1>
        <p className="mb-6 text-sm text-ink-muted">
          Choose a native provider, set host and API key (stored in the OS
          keychain), then create sessions and stream turns.
        </p>

        <ProviderSettingsForm />

        <div className="mt-8 flex flex-wrap gap-3 text-xs text-ink-faint">
          <span>
            <Kbd>Enter</Kbd> send
          </span>
          <span>
            <Kbd>⌘</Kbd> + <Kbd>N</Kbd> new agent
          </span>
          <span>
            <Kbd>⌘</Kbd> + <Kbd>K</Kbd> search
          </span>
          <span>
            <Kbd>Esc</Kbd> cancel turn
          </span>
        </div>
      </div>
    </div>
  )
}
