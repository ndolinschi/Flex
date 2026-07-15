# Desktop UI — Design layout & spacing

How surfaces are composed, where elements sit, and which spacing/size
recipes to reuse. Companion to [COMPONENTS.md](./COMPONENTS.md) (component
catalog). Prefer this file when changing padding, gutters, chrome heights,
or page layout.

Tokens live in `src/styles/tokens.css`. Width clamps live in
`src/stores/layoutConstants.ts`. Feel: compact density, quiet chrome,
whisper fills — never inflate gutters without updating this doc.

Agents: use the **design-audit** skill (`.claude/skills/design-audit`) to
audit and fix UI against this file.

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

---

## App shell

```
┌─ WindowTitleBar (30px) ──────────────────────────────────────┐
│ traffic / captions │ menus │ drag region │                   │
├─ body (flex-1, relative) ────────────────────────────────────┤
│ ┌ SessionSidebar ┐ ┌ Chat column (flex-1) ┐ ┌ RightPanel ┐ │
│ │                │ │ AppHeader (30px)     │ │ TabStrip   │ │
│ │ actions        │ │ TurnTimeline         │ │ tab body   │ │
│ │ session list   │ │ Composer             │ │            │ │
│ │ footer         │ │                      │ │            │ │
│ └────────────────┘ └──────────────────────┘ └────────────┘ │
│   When RightPanel closed: RightPanelMiniTabs flyout (right)  │
└──────────────────────────────────────────────────────────────┘
```

Composition root: `src/App.tsx`.

| Layer | Role |
|---|---|
| `WindowTitleBar` | Custom chrome (`decorations: false`); `--titlebar-height` |
| `SessionSidebar` | Agents list; left column (wide) or full overlay (narrow/tight) |
| Chat column | `ChatPage` kept mounted; settings routes overlay it |
| `RightPanel` | Details pane; sibling of chat column |
| `RightPanelMiniTabs` | Closed-panel mini rows (chat only); no section chrome |
| Overlays | CommandPalette, SearchModal, ToastHost — app-level |

**Chat stays mounted** when opening Settings / Customize / Memory /
Automations (`opacity-0` + absolute settings pane) so timeline subscriptions
survive.

---

## Pages & routes

| Route | Page | Layout |
|---|---|---|
| `welcome` | `WelcomePage` | Title bar + centered rail (`--welcome-rail`); no sidebars |
| `chat` | `ChatPage` → `ChatShell` | Sidebar + header + timeline + composer + right panel |
| `settings` | `SettingsPage` → `SettingsShell` | Absolute overlay over chat; back header + sticky nav + cards |
| `customize` / `memory` / `automations` | Same shell, different section | Same as settings |

### Welcome

- Rail: `max-w-[var(--welcome-rail)]` (28rem) · `px-4 py-8`
- Step forms: `max-w-md` · `gap-3`
- Primary controls: **`h-9`** inputs / `Button size="lg"`
- Cards: `rounded-[var(--radius-card)]` · `px-3.5 py-3`

### Chat (`ChatShell`)

```
AppHeader
main
  ├── timeline (flex-1) — hidden in composer-hero empty state
  └── composer stack (shrink-0, pb-2)
        ├── optional HITL / workers above bubble
        └── Composer
```

- Wide: chat pane `min-w-[380px]` (`CHAT_MIN_WIDTH`)
- Tight: `--content-rail: 100%`; hero/overlay wrappers `px-3` (timeline +
  composer outer wrappers stay `px-4`)

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

| Viewport | Width | Sidebar / right panel |
|---|---|---|
| `wide` | ≥ 940px | Side-by-side columns; resizable sashes |
| `narrow` | 680–939px | Full-height overlays (`z-30` + backdrop `z-20`); auto-collapse on enter |
| `tight` | < 680px | Same overlays as narrow + tighter chat gutters |

**Wide widths** (Zustand + `layoutConstants.ts`):

| Pane | Default | Min | Max |
|---|---|---|---|
| Sidebar | 260px | 210 | 400 |
| Right panel | 380px | 300 | 960 |
| Chat | fluid | **380px** floor | — |

Sashes never shrink chat below `CHAT_MIN_WIDTH` when both side panes are open.

**Narrow/tight overlays:**

- Sidebar: `absolute inset-y-0 left-0 z-30 w-full shadow-popover`
- Right panel: `absolute inset-y-0 right-0 z-30 shadow-popover`
- Backdrop: `absolute inset-0 z-20 bg-black/30`
- Mutual exclusion: opening one closes the other
- Esc order: HITL → sidebar → right panel → cancel turn

**Right panel body:** tab content uses `absolute inset-0` under the strip so
Browser / Terminal always fill remaining height.

**Closed-panel mini tabs:** when `rightPanelOpen` is false on the chat route
(with an active session), `RightPanelMiniTabs` anchors to the right edge of
the chat+panel row (`absolute right-2`, below `--header-height`). Borderless
row stack (`w-[200px]`, no shadow/section labels/expand control); rows use
`fill-2` selected / `fill-4` hover. Not a third panel state — ⌘J / PanelRight
still owns open/close; the flyout only appears while closed.

---

## Spacing canon

Use these gutters unless a surface documents an exception.

| Surface | Horizontal | Vertical / rhythm |
|---|---|---|
| **Chat chrome** (AppHeader, timeline, composer outer) | `px-4` (16px) | Timeline `py-3`; composer `pt-1.5 pb-0.5`; stack `pb-2` |
| **Right panel chrome** (TabStrip, tab headers, banners, CommitCenter) | `px-2.5` (10px) | Rows = `--header-height` (30px) |
| **Session sidebar** (actions, list, section headers) | `px-2` (8px) | Actions `pt-2 pb-2 gap-0.5`; sections `gap-2` |
| **Sidebar footer** | `px-2.5` | `py-1.5` |
| **Composer toolbar / textarea** | `px-2.5` | Toolbar `pt-1 pb-1.5`; textarea `pt-2 pb-1` |
| **Settings shell** | `px-4` | Nav↔content `gap-6`; cards `gap-3` |
| **Settings rows / card labels** | `px-3.5` | Rows `py-3`; dividers `before:inset-x-3.5` |
| **Welcome** | `px-4` | `py-8`; form `gap-3` |
| **Tight hero/overlay only** | `px-3` | — |

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
| `--header-height` | 30px | AppHeader, TabStrip, all right-panel tab headers |
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

`h-[var(--header-height)] px-4` · left: sidebar toggle + title · right:
panel toggle + session menu. Quiet `h-6` icon buttons.

### Composer

1. Outer `px-4` → rail `max-w-[var(--content-rail)]`
2. Optional `workersSlot` / HITL docked flush above the bubble
3. ContextBar above bubble (`mb-1`, min-height status bar)
4. Bubble: `--radius-composer`, shadow-composer
5. Textarea + toolbar both `px-2.5`

### TurnTimeline

Scroll `px-4 py-3` → rail `max-w-[var(--content-rail)] pb-2`. Virtual rows
are `absolute` with padding-based gaps. Live tail (Working, reconnect,
FilesChangedCard) sits **outside** the virtual window. Scroll-down FAB:
`absolute bottom-3 left-1/2`.

### RightPanel

1. **TabStrip** — `px-2.5 gap-1.5`, tabs `h-6`
2. Tab chrome rows — same `px-2.5` / 30px height
3. Body — `relative flex-1` + absolute tab hosts
4. Terminal / Database — optional **180px** left list (`px-2.5 py-1.5 text-xs` rows)

| Tab | Header notes |
|---|---|
| Plan | `PlanToolbar` breadcrumbs + Build (`h-6` controls) |
| Changes | Quiet title row; select toolbar `h-7` (dedicated row, not `--header-height`); file list `px-2` / rows `px-2.5` |
| Files | Open-buffer chips (`Tab` sm) + Monaco / explorer |
| Terminal | Title + New / List; agent subtitle separate bordered row |
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
| Sidebar / right panel overlay + backdrop | `absolute` on app body | `z-30` / `z-20` |
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
| Keep panel chrome at `px-2.5`; chat at `px-4` | Mix gutters under the same strip |
| Put only **`h-6`** controls in 30px header rows | `h-7` pills inside `--header-height` (reads flush) |
| Give TabStrip the bottom border; content headers title the body | Stack two `border-b` with no content between |
| Neutral `stroke-2` focus rings on chrome inputs | Accent glow focus |
| `fill-2` selected / `fill-4` list hover | One-off active fills (`fill-3` as selected) |
| Quiet sash hover (white-alpha) | Accent-colored resize lines |
| Timeline gaps via **padding** | Margin between virtualized rows |
| Keep Chat mounted under settings overlays | Remount ChatPage on settings round-trips |
| Update this file when gutters change | Leave docs stale after a spacing PR |

**Agent skill:** `.claude/skills/design-audit/SKILL.md` — run that procedure for spacing / layout audits.

---

## Checklist for UI changes

1. Pick the surface gutter from **Spacing canon** (chat `px-4`, panel `px-2.5`, sidebar `px-2`).
2. Keep header rows at `--header-height`; controls inside them at **`h-6`**.
3. Align nested chrome with the parent strip (don’t mix `px-2` under a `px-2.5` TabStrip).
4. Prefer tokens / shared atoms (`Tab`, `TabStrip`, `IconButton`) over one-off heights.
5. Update this file when introducing a new page, gutter, or chrome height.
6. Component add/rename → also update [COMPONENTS.md](./COMPONENTS.md).
7. For a full pass, follow the **design-audit** skill checklist and report violations → fixes → exceptions.
