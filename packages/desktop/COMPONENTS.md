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
| `Kbd` | Keyboard shortcut hint | `children` | WelcomePage |
| `Divider` | Horizontal rule | `label?` | SettingsShell |
| `Skeleton` | Placeholder shimmer | `className` | SessionSidebar, TurnTimeline |
| `ScrollArea` | Scrollable region | `children`, `className` | SessionSidebar, TurnTimeline |

## Molecules

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `FormField` | Label + control + hint/error | `label`, `htmlFor`, `error?`, `hint?` | ProviderSettingsForm |
| `SessionListItem` | Agent row + rename/delete + running dot | `session`, `isActive`, `isRunning?` | SessionSidebar |
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
| `MarkdownBody` | GFM + highlight.js | `content` | TurnTimeline |
| `EmptyState` | Empty async surface | `title`, `description?`, `action?` | SessionSidebar, TurnTimeline |
| `ErrorBanner` | Inline error | `message`, `onDismiss?` | Composer, Settings |
| `ToolCallChip` | Single tool as Cursor-style step | `call` | TurnTimeline |
| `ToolStepGroup` | Aggregated explore/edit/shell summary + card expand | `calls` | TurnTimeline (via ToolStepList) |
| `StreamingCaret` | Streaming caret | — | TurnTimeline |
| `SubagentGroup` | Nested subagent work block | `task`, `role?`, `phase` | TurnTimeline |
| `WorkGroup` | "Worked for Xs" / "Working" | `isOpen`, `durationMs?` | TurnTimeline |
| `SidebarActionRow` | New Agent / Search row | `icon`, `label`, `kbd?` | SessionSidebar |
| `RepoSectionHeader` | Collapsible repo group | `label`, `collapsed`, `onToggle` | SessionSidebar |

## Organisms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `SessionSidebar` | New Agent + Search + Agents list + theme/settings footer | (hooks) | App shell |
| `ProviderSettingsForm` | Provider / key / model | — | SettingsPage, WelcomePage |
| `Composer` | Prompt + ContextBar (project/branch/env) | `isHero?` | ChatShell |
| `ContextBar` | Project · branch · context % | `cwd`, `sessionId` | Composer |
| `TurnTimeline` | Turns + tools + plans + streaming | `sessionId` | ChatShell |
| `PermissionPrompt` | Tool permission HITL | `permission` | ChatPage |
| `QuestionPrompt` | AskUserQuestion HITL | `question` | ChatPage |
| `RightPanel` | Plan / Changes / Terminal / Browser | — | App shell |
| `AppHeader` | Title + session menu | — | ChatShell |

## Templates / Pages

| Component | Purpose |
|---|---|
| `ChatShell` | Header + timeline + composer (`hideSidebar` when App owns sidebar) |
| `SettingsShell` | Back header + form (`embedded` when App owns sidebar) |
| `ChatPage` | Conversation (`embedded`) |
| `SettingsPage` | Provider config (`embedded`) |
| `WelcomePage` | First-run setup |

## Data layer

| Module | Role |
|---|---|
| `src/lib/tauri.ts` | Typed IPC |
| `src/lib/types.ts` | Wire + DTO types |
| `src/lib/browserMock.ts` | Vite preview only — never used under Tauri |
| `src/stores/appStore.ts` | Route, theme, drafts, mode, streaming, questions, recentCwds |
| `src/hooks/useSessions.ts` | Session CRUD |
| `src/hooks/useSessionEvents.ts` | Active-session replay + timeline rows |
| `src/hooks/useGlobalSessionEvents.ts` | App-level `session-event` fan-out + subscribe for streaming sessions |
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
- Engine settings: plugin toggles (search/learning/verifier), fallback models, default isolation.
- Composer `/` opens slash-command tray; SessionMenu supports undo/redo files + integrate/discard when isolated.

Keep this file in sync when adding or renaming components.
