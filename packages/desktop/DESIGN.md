# Desktop UI — Design layout & spacing

How surfaces are composed, where elements sit, and which spacing/size
recipes to reuse. Companion to [COMPONENTS.md](./COMPONENTS.md) (component
catalog). Prefer this file when changing padding, gutters, chrome heights,
or page layout.

Tokens live in `src/styles/tokens.css`. Width clamps live in
`src/stores/layoutConstants.ts`. Feel: compact density, quiet chrome,
whisper fills — never inflate gutters without updating this doc.

Agents: use the **design-audit** skill (`.claude/skills/design-audit`) to
audit and fix UI against this file. For shadcn adds/rewrites, also load the
**shadcn** skill and follow the migration map in [COMPONENTS.md](./COMPONENTS.md).

---

## Feel principles

| Principle | Practice |
|---|---|
| Compact density | 30px chrome rows; prefer `h-6` controls in headers |
| Quiet chrome | Hairline `stroke-3` borders; sash hover is white-alpha, never accent |
| Whisper fills | Selected `fill-2`, hover `fill-4` / `fill-3` |
| Opacity hover | Quiet `IconButton`: idle `.5` → hover `.8` |
| 4px grid | Spacing tokens `--space-1`…`--space-12` (4–48px) |
| Radius by role | Controls `rounded-md` (8); composer/bubbles 14; settings cards 12; pills full |
| Keyboard focus | Neutral `stroke-2` ring; no accent glow |
| Semibold = 590 | Plus micro tracking on captions |

### shadcn token bridge (when `components.json` lands)

Phase 0 of the migration maps shadcn semantic variables onto these Flex tokens
— Flex wins on conflict. Typical aliases (illustrative):

| shadcn semantic | Flex source |
|---|---|
| `--background` / `--card` / `--popover` | `--color-chrome` / `--color-elevated` / `--color-panel` |
| `--foreground` / `--muted-foreground` | `--color-text-1` / `--color-text-2` |
| `--border` / `--input` / `--ring` | `--color-stroke-3` / `--color-stroke-2` (ring stays neutral — never accent glow) |
| `--primary` / `--primary-foreground` | `--color-accent` / `--color-accent-text` |
| `--destructive` | `--color-danger` |
| `--radius` | keep role radii (`--radius-*`); controls stay `rounded-md` (8) |

Do not introduce a second theme system. Keep `data-theme="dark"|"light"` and
Settings → Appearance accent overrides as the only runtime theme knobs.

---

## App shell

```
┌─ WindowTitleBar (30px) ──────────────────────────────────────┐
│ traffic / captions │ menus │ drag region │                   │
├─ body (flex-1, relative) ────────────────────────────────────┤
│ ┌ SessionSidebar ┐ ┌ ContentWorkspace (flex-1) ─────────────┐ │
│ │                │ │ AppHeader (sidebar · split)            │ │
│ │ actions        │ │ ContentPane(s) — tabs + chat/tool body │ │
│ │ session list   │ │   single OR left | sash | right        │ │
│ │ footer         │ │                                        │ │
│ └────────────────┘ └────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

Composition root: `src/App.tsx`.

| Layer | Role |
|---|---|
| `WindowTitleBar` | Custom chrome (`decorations: false`); `--titlebar-height` |
| `SessionSidebar` | Agents list; left column (wide) or full overlay (narrow/tight) |
| `ContentWorkspace` | Header + content panes (chat + tool tabs; optional split) |
| `ContentPane` | Per-pane tab strip + bodies; `+` / open-to-side |
| Overlays | CommandPalette, SearchModal, ToastHost — app-level |

**Chat stays mounted** when opening Settings / Customize / Memory /
Automations (`opacity-0` + absolute settings pane) so timeline subscriptions
survive.

---

## Pages & routes

| Route | Page | Layout |
|---|---|---|
| `welcome` | `WelcomePage` | Title bar + centered rail (`--welcome-rail`); no sidebars |
| `chat` | `ContentWorkspace` | Sidebar + header + content panes (chat + tools; optional split) |
| `settings` | `SettingsPage` → `SettingsShell` | Absolute overlay over chat; back header + sticky nav + cards |
| `customize` / `memory` / `automations` | Same shell, different section | Same as settings |

### Welcome

- Rail: `max-w-[var(--welcome-rail)]` (28rem) · `px-4 py-8`
- Step forms: `max-w-md` · `gap-3`
- Primary controls: **`h-9`** inputs / `Button size="lg"`
- Cards: `rounded-[var(--radius-card)]` · `px-3.5 py-3`

### Chat (`ContentWorkspace`)

```
AppHeader (30px) — sidebar toggle · split toggle (⌘J) · session menu
ContentPane(s)
  ├── TabStrip — chat sessions + tool tabs (+ / open-to-side)
  └── body — ChatSessionBody or tool tab (Plan/Changes/…)
```

- **Single:** one pane fills the column.
- **Split (wide only):** left | sash | right; each pane has its own tabs.
- Auto-open (plan approval, PR, files, browser, terminal) uses `openToolBesideChat`.
- Narrow/tight: force single pane.

### Settings (`SettingsShell`)

```
header (30px, px-4) — Back
main (px-4 gap-6 overflow-y-auto)
  ├── SettingsNav — sticky, width clamp(100px, 25%, 200px)
  └── content
        ├── title (pt-6 mb-5)
        └── sections (gap-3 pb-12)
```

---

## Viewports & positioning

| Viewport | Width | Sidebar / content |
|---|---|---|
| `wide` | ≥ 940px | Side-by-side; optional content split sash |
| `narrow` | 680–939px | Sidebar full-height overlay; content forced single pane |
| `tight` | < 680px | Same overlays as narrow + tighter chat gutters |

**Wide widths** (Zustand + `layoutConstants.ts`):

| Pane | Default | Min | Max |
|---|---|---|---|
| Sidebar | 260px | 210 | 400 |
| Content pane (each, when split) | ~50% | **380px** floor | — |
| Chat body (single) | fluid | **380px** floor | — |

Sashes never shrink a content pane below `CHAT_MIN_WIDTH` when split.

**Narrow/tight overlays:**

- Sidebar: `absolute inset-y-0 left-0 z-30 w-full shadow-popover`
- Backdrop: `absolute inset-0 z-20 bg-black/30`
- Content split is disabled (single pane only)
- Esc order: HITL → sidebar → cancel turn

**Tool tab bodies:** tab content uses `absolute inset-0` under the strip so
Browser / Terminal always fill remaining height.

**Closed-panel mini tabs:** removed — the right column no longer exists.
Tool surfaces open as content tabs (optionally beside chat in split mode).


---

## Spacing canon

Use these gutters unless a surface documents an exception.

| Surface | Horizontal | Vertical / rhythm |
|---|---|---|
| **Chat chrome** (AppHeader, timeline, composer outer) | `px-3` (12px) | Timeline `py-3`; composer `pt-1.5 pb-0.5`; stack `pb-2` |
| **Content pane chrome** (TabStrip, tool tab headers, banners, CommitCenter) | `px-2.5` (10px) | Rows = `--header-height` (30px) |
| **Session sidebar** (actions, list, section headers) | `px-2` (8px) | Actions `pt-2 pb-2 gap-0.5`; sections `gap-2` |
| **Sidebar footer** | `px-2.5` | `py-1.5` |
| **Composer toolbar / textarea** | `px-2.5` | Toolbar `pt-1 pb-1.5`; textarea `pt-2 pb-1` |
| **Settings shell** | `px-4` | Nav↔content `gap-6`; cards `gap-3` |
| **Settings rows / card labels** | `px-3.5` | Rows `py-3`; dividers `before:inset-x-3.5` |
| **Welcome** | `px-4` | `py-8`; form `gap-3` |
| **Tight viewport** | chat chrome stays `px-3` | `--content-rail: 100%` (full column) |

### Content rails

| Token | Value | Used by |
|---|---|---|
| `--content-rail` | 52.5rem (840px) | Timeline + composer `max-w` |
| `--welcome-rail` | 28rem (448px) | Welcome page |
| `--form-rail` | 32rem | Defined; unused in components today |
| `--sidebar-width` | 16.5rem | Defined; runtime width is the store, not this token |

### Chrome heights

| Token | Value | Surfaces |
|---|---|---|
| `--titlebar-height` | 30px | `WindowTitleBar` |
| `--header-height` | 30px | AppHeader, content TabStrip, tool tab subheaders |
| `--status-bar-height` | 1.75rem (28px) | ContextBar min height |
| `--composer-min/max-height` | 1.75rem / 10rem | Textarea grow |

### Gaps (chrome)

| Cluster | Gap |
|---|---|
| AppHeader control clusters | `gap-0.5` |
| TabStrip tabs | `gap-1.5` |
| Composer left (Plus / Mode / Model) | `gap-1` |
| Composer right (Bypass ↔ Send) | `gap-1.5` |
| ContextBar outer | `gap-2` |
| Sidebar action rows | `gap-0.5` |
| Settings cards | `gap-3` |
| Settings shell columns | `gap-6` |

### Timeline row spacing

Use **padding** (`pt-*`), not margin — virtualizer `measureElement` must include gaps.

| Item kind | Top padding |
|---|---|
| User message | `pt-3` |
| Assistant | `pt-2` |
| Work / tool groups | `pt-1.5` |
| Default | `pt-1` |

---

## Control size recipes

| Control | Default | In 30px chrome |
|---|---|---|
| `IconButton` | `h-7 w-7` | Override **`h-6 w-6`**; icon `h-3.5 w-3.5` |
| `Button` sm / md / lg | `h-7` / `h-8` / `h-9` | Prefer `h-6` override in panel headers |
| Composer Plus / Bypass / Send | — | **`h-6 w-6`** circles |
| Mode / Model pills | — | **`h-6`** `rounded-full px-2` |
| `Tab` md (panel) | **`h-6`** `px-2 rounded-md text-sm` | Must clear strip edges |
| `Tab` sm (file chips) | **`h-6`** tighter pad, `text-xs` | Same strip |
| `TextInput` | `h-8` | Settings search `h-7`; Welcome `h-9` |
| Sidebar session row | `min-h-7` `px-2 py-1.5` | Status slot `h-5 w-5` |
| Section headers (sidebar) | `h-6` `px-2` | — |

**Rule:** never put `h-7` pills inside a `--header-height` (30px) bar — they
read flush against the border. Use `h-6` (3px inset each side).

---

## Per-surface layout

### SessionSidebar

1. Optional narrow close header (`px-4`, 30px)
2. Action rows — New Agent, Search, … (`px-2 pt-2 pb-2`)
3. “Repositories” label (`px-2 pb-1`)
4. Scrollable groups — Pinned / repos / Archived (`px-2`)
5. Footer — theme + settings (`px-2.5 py-1.5 border-t`)

Selected row: `bg-fill-2`. Hover: `bg-fill-4`.

### AppHeader

`h-[var(--header-height)] px-3` · left: sidebar toggle · center: scrollable
`ChatSessionTabBar` (`flex-1 min-w-0 overflow-x-auto`, no second border) ·
right: panel toggle + session menu. Quiet `h-6` icon buttons.

### Composer

1. Outer `px-3` → rail `max-w-[var(--content-rail)]`
2. Optional `workersSlot` / HITL docked flush above the bubble
3. ContextBar above bubble (`mb-1`, min-height status bar)
4. Bubble: `--radius-composer`, shadow-composer
5. Textarea + toolbar both `px-2.5`

### TurnTimeline

Scroll `px-3 py-3` → rail `max-w-[var(--content-rail)] pb-2`. Virtual rows
are `absolute` with padding-based gaps. Live tail (Working, reconnect,
FilesChangedCard) sits **outside** the virtual window. Scroll-down FAB:
`absolute bottom-3 left-1/2`.

### RightPanel

1. **TabStrip** — `px-2.5 gap-1.5`, tabs `h-6`
2. Tab chrome rows — same `px-2.5` / 30px height
3. Body — `relative flex-1` + absolute tab hosts
4. Terminal / Database / Components — optional **180px** left list (`px-2.5 py-1.5 text-xs` rows)

| Tab | Header notes |
|---|---|
| Plan | `PlanToolbar` breadcrumbs + Build (`h-6` controls) |
| Changes | Quiet title row; select toolbar `h-7` (dedicated row, not `--header-height`); file list `px-2` / rows `px-2.5` |
| Pull Request | Title / # / state / checks; Open in browser; DiffView of `gh pr diff` (tab only when branch has a PR) |
| Files | Open-buffer chips (`Tab` sm) + Monaco / explorer |
| Terminal | Title + New / List; agent subtitle separate bordered row |
| Components | Count + List/Refresh; Files-style open chips; bottom mini-prompt + Send |
| Browser | Toolbar `z-20` over webview slot |
| Database | Connection count chrome; schema chips `py-1.5` |

### Settings

Nav sticky `pt-6`. Cards use `--radius-card` + `bg-settings-card`. Rows
`px-3.5 py-3 gap-4`. Field grids switch at `@container` 640px.

---

## Overlay z-index

| Surface | Positioning | z |
|---|---|---|
| CommandPalette / SearchModal | `fixed inset-0`; panel `mt-[10vh] w-[560px]` | `z-[300]` |
| Sidebar overlay + backdrop | `absolute` on app body | `z-30` / `z-20` |
| Composer stack / HITL | In-flow above bubble; ChatShell slot `z-50` | — |
| SubagentViewer | Bottom sheet over timeline `main` | `z-10`–`z-20` |
| Scroll-to-bottom | Absolute in timeline | `z-20` |
| Tooltips / context menus | Portaled | ≥ `z-[1100]` tooltips |

Native Browser webview stacks above DOM — use
`data-suppress-native-webview` / `aria-modal` intersection (see
`nativeWebviewGate.ts`) when a modal must cover it.

---

## Spacing scale (tokens)

| Token | rem | px |
|---|---|---|
| `--space-1` | 0.25 | 4 |
| `--space-2` | 0.5 | 8 |
| `--space-3` | 0.75 | 12 |
| `--space-4` | 1 | 16 |
| `--space-5` | 1.25 | 20 |
| `--space-6` | 1.5 | 24 |
| `--space-8` | 2 | 32 |
| `--space-10` | 2.5 | 40 |
| `--space-12` | 3 | 48 |

Tailwind `p-*` / `gap-*` map through `@theme` in `src/index.css`.

---

## Do / Don’t (from design-system audits)

| Do | Don’t |
|---|---|
| Reuse `Tab` / `TabStrip` / `TabClose` for panel tabs + file chips | Duplicate pill markup per surface |
| Keep content-pane / tool chrome at `px-2.5`; chat at `px-3` | Mix gutters under the same strip (`px-2` under a `px-2.5` TabStrip) |
| Put only **`h-6`** controls in 30px header rows | `h-7` pills inside `--header-height` (reads flush) |
| Give TabStrip the bottom border; content headers title the body | Stack two `border-b` with no content between |
| Neutral `stroke-2` focus rings on chrome inputs | Accent glow focus |
| `fill-2` selected / `fill-4` list hover | One-off active fills (`fill-3` as selected) |
| Quiet sash hover (white-alpha) | Accent-colored resize lines |
| Timeline gaps via **padding** | Margin between virtualized rows |
| Keep Chat mounted under settings overlays | Remount ContentWorkspace on settings round-trips |
| Update this file when gutters change | Leave docs stale after a spacing PR |

**Agent skill:** `.claude/skills/design-audit/SKILL.md` — run that procedure for spacing / layout audits.

---

## Checklist for UI changes

1. Pick the surface gutter from **Spacing canon** (chat `px-3`, content pane `px-2.5`, sidebar `px-2`).
2. Keep header rows at `--header-height`; controls inside them at **`h-6`**.
3. Align nested chrome with the parent strip (don’t mix `px-2` under a `px-2.5` TabStrip).
4. Prefer tokens / shared atoms (`Tab`, `TabStrip`, `IconButton`) over one-off heights.
5. Update this file when introducing a new page, gutter, or chrome height.
6. Component add/rename → also update [COMPONENTS.md](./COMPONENTS.md).
7. For a full pass, follow the **design-audit** skill checklist and report violations → fixes → exceptions.
