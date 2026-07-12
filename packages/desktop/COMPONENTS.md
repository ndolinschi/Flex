# Desktop UI — Atomic Component Catalog

Atomic Design catalog for `packages/desktop`. Presentation components are dumb;
data lives in hooks (`src/hooks/`) and Zustand (`src/stores/`).

## Atoms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `Button` | Primary action control | `variant`, `size`, `disabled`, `onClick`, `children` | Composer, ProviderSettingsForm, PermissionPrompt, QuestionPrompt, WelcomePage, ConfirmDialog |
| `IconButton` | Compact icon-only action; optional `quiet` opacity .5→.8 | `label`, `quiet?`, `onClick`, `children` | SessionListItem, SessionSidebar, PlusMenu, ErrorBanner, SettingsShell, SessionMenu |
| `TextInput` | Single-line text field (forwardRef) | standard input props | FormField, SessionListItem, SessionSidebar search, QuestionPrompt, ConfirmDialog |
| `TextArea` | Multi-line text field | standard textarea props | Composer |
| `Label` | Accessible form label | `htmlFor`, `children` | FormField, ModelSelect |
| `Spinner` | Indeterminate loading | `size` | SessionSidebar, ProviderSettingsForm |
| `Badge` | Status / meta chip | `tone`, `children` | ToolCallChip |
| `BypassPermissionsButton` | Session bypass-permissions shield | `composerMode`, `sessionBypass`, `onToggle` | Composer |
| `Kbd` | Keyboard shortcut hint | `children` | WelcomePage |
| `Divider` | Horizontal rule | `label?` | SettingsShell |
| `HighlightedLabel` | Fuzzy-match accent spans in a label | `label`, `query` | FuzzySessionRow |
| `Skeleton` | Placeholder shimmer | `className` | SessionSidebar, TurnTimeline |
| `ScrollArea` | Scrollable region | `children`, `className` | SessionSidebar, TurnTimeline |

## Molecules

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `FormField` | Label + control + hint/error | `label`, `htmlFor`, `error?`, `hint?` | ProviderSettingsForm |
| `CommandPaletteRow` | Palette list row (icon + label + hint) | `index`, `active`, `label`, `hint?`, `icon?`, `onActivate`, `onHover` | CommandPalette |
| `FuzzySessionRow` | Search-modal session row with highlight + relative time | `index`, `active`, `label`, `query`, `updatedAtMs`, `onActivate`, `onHover` | SearchModal |
| `ProviderProfileList` | Connections list (select / activate / delete) | `profiles`, `editingId`, `onSelect`, … | ProviderSettingsForm |
| `ProviderConnectionForm` | Connection create/edit form + models + isolation | form field props + `onValidate` / `onSave` | ProviderSettingsForm |
| `SecretStorageSection` | Security: secret-storage backend select | `secretStorage`, `isMac`, `onChange`, `error?` | ProviderSettingsForm |
| `SessionListItem` | Agent row + rename/delete + running/unread via per-id store selectors | `session`, `isActive`, `memo` | SessionSidebar |
| `SessionRowSubtitle` | Diff + relative-time under a session title | `updatedAtMs`, `workspaceStatus?`, `gitStatus?` | SessionListItem |
| `SessionRowActions` | Hover pin / archive / more trailing actions | `pinned`, `archived`, `onTogglePin`, … | SessionListItem |
| `SidebarFooter` | Theme + settings chrome (+ optional creating spinner) | `theme`, `onToggleTheme`, `onOpenSettings`, `isCreating?` | SessionSidebar |
| `SidebarResumeError` | Resume-failure Retry / Dismiss banner | `message`, `onRetry`, `onDismiss` | SessionSidebar |
| `ArchivedSectionHeader` | Collapsible Archived group header | `count`, `collapsed`, `onToggle` | SessionSidebar |
| `ComposerInput` | Draft-subscribed textarea + backdrop + slash/@ trays (isolates keystrokes from ModelPicker/ContextBar) | `composerMode`, `anchorRef`, `attachments`, `onSend` | Composer |
| `ModelSelect` | Simple model `<select>` | `models`, `value`, `onChange` | ProviderSettingsForm |
| `ModelPicker` | Searchable model tray (PopoverTray) | `models`, `value`, `onChange` | Composer |
| `ModePicker` | Agent / Plan / Ask pill switcher | `value`, `onChange` | Composer |
| `PlanBuildBar` | Cursor-style Build CTA after ExitPlanMode | `onBuild`, `onKeepPlanning?`, `variant` | RightPanel Plan tab, ChatPage |
| `PlanCard` | Checklist from `plan_updated` (right Plan tab; not inlined in timeline) | `entries` | RightPanel |
| `PlusMenu` | Attach + mode shortcuts (Plan/Ask) | `onAttachFile`, `onAttachImage`, `onSetMode?` | Composer |
| `ProjectPicker` | Recent cwds + Open Folder | `sessionId`, `cwd`, `onError?` | ContextBar |
| `BranchPicker` | List/checkout local git branches | `cwd`, `onError?` | ContextBar |
| `PopoverTray` | Shared Esc/click-outside/↑↓ tray | `open`, `onClose`, `placement`, `children` | Model/Mode/Plus/Project/Branch pickers |
| `ConfirmDialog` | In-app modal (rename/delete) | `open`, `title`, `onConfirm`, `onCancel` | SessionMenu |
| `AttachmentChip` | Pending attachment pill | `attachment`, `onRemove` | Composer |
| `SendButton` | Circular send / stop / queue | `isStreaming`, `canQueue?`, `onSend`, `onStop` | Composer |
| `MarkdownBody` | GFM + lazy highlight.js language pack; `live` plain pre-wrap fast-path | `content`, `live?` | TurnTimeline (`TimelineRowView`) |
| `EmptyState` | Empty async surface | `title`, `description?`, `action?` | SessionSidebar, TurnTimeline |
| `ErrorBanner` | Inline error | `message`, `onDismiss?` | Composer, Settings |
| `ToolCallChip` | Single tool as Cursor-style step | `call` | TurnTimeline |
| `ToolStepGroup` | Aggregated explore/edit/shell summary + card expand | `calls` | TurnTimeline (via ToolStepList) |
| `ToolStepList` | Clusters consecutive same-kind tool rows | `rows`, `renderOther` | TurnTimeline |
| `DetailRow` / `BackgroundBashRow` / `ExecTail` | Tool-step detail / background bash / exec tail | — | ToolStepGroup |
| `StreamingCaret` | Streaming caret | — | TurnTimeline |
| `SubagentGroup` | Nested subagent work block | `task`, `role?`, `phase` | TurnTimeline |
| `WorkGroup` | "Worked for Xs" / live "Working" XOR "Thinking"; `memo` | `isOpen`, `liveStatus?`, `durationMs?` | TurnTimeline |
| `WorkflowGroup` | Multi-step workflow block (steps + nested subagents); organism-scale (261 lines) but kept in `molecules/` since it nests inside `TimelineRowView` like `SubagentGroup`/`WorkGroup` | `steps`, `subagents`, `status` | TurnTimeline (via `TimelineRowView`) |
| `SidebarActionRow` | New Agent / Search row | `icon`, `label`, `kbd?` | SessionSidebar |
| `RepoSectionHeader` | Collapsible repo group | `label`, `collapsed`, `onToggle` | SessionSidebar |
| `PlanToolbar` | Plan tab header: model/mode pickers + build actions | `value`, `onChange`, `status` | RightPanel Plan tab (`PlanTab`) |

## Organisms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `SessionSidebar` | New Agent + Search + Agents list; groups via `useSessionSidebarGroups`; footer/resume/archive molecules | (hooks) | App shell |
| `ProviderSettingsForm` | Provider / key / model; pieces: `ProviderProfileList`, `ProviderConnectionForm`, `SecretStorageSection` | — | SettingsPage, WelcomePage |
| `Composer` | Prompt + ContextBar; draft in `ComposerInput`; trays/queue under `organisms/composer/` | `isHero?` | ChatShell |
| `ContextBar` | Project · branch · context % | `cwd`, `sessionId` | Composer |
| `TurnTimeline` | Turns + tools + plans + streaming; `@tanstack/react-virtual` over `displayItems` + live tail; pieces under `organisms/timeline/` (`WorkGroupBody` owns stable `renderOther`) | `sessionId` | ChatShell |
| `PermissionPrompt` | Tool permission HITL | `permission` | ChatPage |
| `QuestionPrompt` | AskUserQuestion HITL | `question` | ChatPage |
| `RightPanel` | Plan / Changes / Terminal / Browser; tabs under `organisms/right-panel/` (`RightPanelTabBar`, `tabs`) | — | App shell |
| `AppHeader` | Title + session menu | — | ChatShell |
| `BrowserTab` | Embedded browser panel; chrome under `organisms/browser/` | `active` | RightPanel |
| `TerminalTab` | PTY / agent terminal; pieces under `organisms/terminal/` | — | RightPanel |
| `CommandPalette` | ⌘K-style action palette (nav, theme, new agent); rows via `CommandPaletteRow`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell |
| `SearchModal` | Fuzzy session search overlay; rows via `FuzzySessionRow` + `HighlightedLabel`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell (via SessionSidebar's `onOpenSearch`) |
| `SubagentViewer` | Bottom-anchored overlay replaying a subagent's inner session feed | (reads `useAppStore` `subagentViewer`) | App shell; opened from `TimelineRowView` |

### Organism subfolders

| Path | Contents |
|---|---|
| `organisms/timeline/` | `buildDisplayItems` (+ `estimateSizeForItem`), `TimelineRowView`, `WorkGroupBody`, `ThinkingBlock`, `MessageActions`, `TurnFooter`, `ReconnectBanner`, `CheckpointChip` |
| `organisms/composer/` | `SlashCommandTray`, `AtMentionTray`, `ComposerQueue`, `composerAttachments` |
| `organisms/right-panel/` | `PlanTab`, `ChangesTab`, `FileRow`, `CommitCenter`, `RightPanelTabBar`, `tabs` |
| `organisms/browser/` | `BrowserToolbar`, `BrowserOverflowMenu` — composed by `BrowserTab` |
| `organisms/terminal/` | `TerminalTab`, `TerminalInstance`, `TerminalRow`, `AgentTerminalRow`, `time` helpers |

## Templates / Pages

| Component | Purpose |
|---|---|
| `ChatShell` | Header + timeline + composer (`hideSidebar` when App owns sidebar) |
| `SettingsShell` | Back header + form (`embedded` when App owns sidebar) |
| `ErrorBoundary` | Top-level render-error fence (`templates/`) |
| `ChatPage` | Conversation (`embedded`) |
| `SettingsPage` | Settings shell; sections from `pages/settings/` |
| `CustomizeSection` / `MemorySection` / `IndexingSection` / `AutomationsSection` / `DiagnosticsSection` | Settings nav sections (`pages/settings/`) |
| `WelcomePage` | First-run wizard: provider key → model → optional project |

### Page subfolders

| Path | Contents |
|---|---|
| `pages/settings/customize/` | `PluginCatalog`, `McpCatalogSection`, `McpServersSection`, `McpServerRow`, `CreateMcpServerForm` — composed by `CustomizeSection` |
| `pages/settings/automations/` | `AutomationsContent`, `CreateRoutineForm`, `RoutineRow`, `TriggerSummary`, `constants` — composed by `AutomationsSection` |
| `pages/settings/memory/` | `MemoryContent`, `MemoryRow`, `ExpiryPill`, `MemoryScopeSection`, `ProjectMemorySection`, `useProjectCwds`, `constants` — composed by `MemorySection` |

## Data layer

| Module | Role |
|---|---|
| `src/lib/updater.ts` | tauri-plugin-updater check/install helpers (GitHub Releases channel) |
| `src/lib/tauri.ts` | Typed IPC |
| `src/lib/types/` | Wire + timeline + UI types (`wire.ts`, `timeline.ts`, `ui.ts`, barrel `index.ts`) |
| `src/lib/timeline/` | Pure timeline fold: `applyEvent`, `applyStreaming`, `parseWorkflow`, `thinkingSpans`, `rowIds` |
| `src/lib/toolPresentation.ts` | Pure tool classify/summarize/cluster helpers |
| `src/lib/sessionSideEffects/` | Global-event side effects (`applyGlobalEvent`, `agentTerminal`, `devServerToast`) |
| `src/lib/browserMock.ts` | Vite preview only — never used under Tauri |
| `e2e/` + `playwright.config.ts` | Phase 3.1 PR-gate Playwright smoke (vite + browserMock) |
| `scripts/soak.mjs` | Phase 3.2 nightly soak skeleton (N mock turns + memory samples) |
| `scripts/preview-verify.mjs` | Manual screenshot walk against a running `pnpm dev` |
| `src/lib/mcp.ts` | Pure MCP form helpers: `parseArgs`, `parseEnv`, `MCP_ID_RE`, `buildCatalogServerDto` |
| `src/lib/sessionGrouping.ts` | Pure `groupByRepo` — groups/sorts sessions by `cwd` for SessionSidebar |
| `src/lib/fuzzySearch.ts` | Shared `fuzzyScore` / `fuzzyMatchIndices` for CommandPalette + SearchModal |
| `src/lib/markdownHighlight.ts` | Lazy-loaded rehype-highlight + core language subset (dynamic import from `MarkdownBody`) |
| `src/stores/appStore.ts` | Composes Zustand slices; public `useAppStore` API |
| `src/stores/slices/` | `session` / `composer` / `layout` / `ui` / `panelExtras` slices |
| `src/stores/persist.ts` | `persistUiState` / `restoreUiState` |
| `src/stores/layoutConstants.ts` | Sidebar / right-panel / chat width clamps |
| `src/hooks/useUpdaterCheck.ts` | Post-bootstrap update toast |
| `src/hooks/useSessions.ts` | Session CRUD |
| `src/hooks/useSessionSidebarGroups.ts` | Pin/archive/repo grouping + stable order for SessionSidebar |
| `src/hooks/useSessionEvents.ts` | Active-session replay + timeline rows (thin over `lib/timeline`) |
| `src/hooks/useLatestVerdict.ts` | Narrow per-session latest Verify verdict (Plan tab) |
| `src/hooks/useGlobalSessionEvents.ts` | App-level `session-event` fan-out + subscribe |
| `src/hooks/useComposerSend.ts` | Subscribe-wait + send/queue constants |
| `src/hooks/useStickToBottom.ts` | Timeline stick-to-bottom scroll (narrow `streamContentKey` dep) |
| `src/hooks/useGroupedModels.ts` | Model picker grouping |
| `src/hooks/useKeyboardShortcuts.ts` | Enter / ⌘N / ⌘K / ⌘L / Esc |
| `src/hooks/useProviderConfig.ts` | Provider + plugin + fallback prefs |

## Theme & motion

- Themes: `data-theme="dark"|"light"` on `<html>` (Cursor Glass palettes).
- Feel principles (local `design-map/README.md`): compact density, quiet chrome, whisper fills, opacity hover, micro-motion, alpha hierarchy, keyboard focus, weight 590 + micro tracking, alpha borders, radius-by-role, neutral interactive chrome, thin scrollbars.
- Motion: hover 100ms ease; trays `animate-tray-in`; pane swaps `animate-pane-fade`; timeline rows `animate-row-fade`; end-of-turn `animate-end-turn-in` (160ms); HITL cards `animate-modal-in` (scale .97→1); overlays `animate-backdrop-in`.
- Composer: `--radius-composer` 14px, soft elevation + stroke focus (no accent glow), auto-grow 36–200px; quiet toolbar opacity (0.5→0.8); mode/model pills neutral fill + stroke.
- Content rail: `--content-rail` 840px (`52.5rem`).
- ContextBar sits above the composer (project / branch / context %) — Flex Canon.
- Sidebar footer = theme + settings (Flex Canon); rows use fill-4 hover / fill-2 selected.
- Right panel tabs = Plan / Changes / Terminal / Browser (Flex Canon); pill tabs; sash hover white-alpha.
- Prior user bubbles dim to 50% (hover restores); hairline stroke-2; message actions reveal on row hover.
- Sessions: default title `New Agent`; one draft per project; first prompt renames the session.
- Engine settings: plugin toggles (search/index/learning/verifier), Indexing section
  (status/rebuild/auto-context), fallback models, default isolation.
- Composer `/` opens slash-command tray; SessionMenu supports undo/redo files + integrate/discard when isolated.

Keep this file in sync when adding or renaming components.

## Perf notes (Wave 3)

- **Timeline virtualization:** `TurnTimeline` uses `@tanstack/react-virtual` over `displayItems`. Live tail (Working / reconnect / FilesChangedCard / bottom sentinel) stays outside the virtual window so stick-to-bottom remains correct. Virtualized rows do **not** use `content-visibility: auto` — cv on the mounted overscan window races with WebView2 measurement during scroll (Windows overlap). Off-screen work is already skipped by unmounting. Item spacing uses padding (`pt-*`) so virtual `measureElement` includes gaps; `translateY` offsets are rounded to integer px for fractional DPI.
- **React Compiler:** Enabled in `vite.config.ts` via `babel-plugin-react-compiler` (React 19 target). Verified with `tsc --noEmit`, `vitest run`, and `vite build`.
- **Markdown highlight:** Core language pack loads as a separate chunk (`lib/markdownHighlight.ts`); GFM renders immediately, highlight upgrades after the dynamic import.
