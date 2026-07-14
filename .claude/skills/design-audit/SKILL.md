---
name: design-audit
description: Audit and fix Flex desktop UI spacing, gutters, chrome heights, and layout against packages/desktop/DESIGN.md. Use when the user asks for a design audit, spacing/padding/slop fixes, layout polish, tab/chrome flush issues, or to develop UI with design best practices.
---

# Design audit (desktop)

Source of truth: [`packages/desktop/DESIGN.md`](../../../packages/desktop/DESIGN.md).
Component catalog: [`packages/desktop/COMPONENTS.md`](../../../packages/desktop/COMPONENTS.md).
Tokens: `packages/desktop/src/styles/tokens.css`.
Width clamps: `packages/desktop/src/stores/layoutConstants.ts`.

Do **not** invent a parallel design-map. If comments still say `design-map/…`, point them at `DESIGN.md` instead.

## When to run

- User mentions design audit, spacing, padding, margins, “slop”, flush tabs, tight chrome
- After adding a page, panel tab, header row, or gutter
- Before shipping desktop UI polish PRs

## Canon (must match DESIGN.md)

| Surface | Horizontal gutter | Chrome height / controls |
|---|---|---|
| Chat (AppHeader, timeline, composer outer) | `px-4` | `--header-height` 30px; controls **`h-6`** |
| Right panel chrome (TabStrip, tab headers, banners) | `px-2.5` | TabStrip `gap-1.5`; `Tab` md/sm **`h-6`** |
| Session sidebar list / actions | `px-2` | Rows `min-h-7`; section headers `h-6` |
| Sidebar footer | `px-2.5` | `py-1.5` |
| Composer toolbar / textarea | `px-2.5` | Plus / Bypass / Send / Mode / Model **`h-6`** |
| Settings shell | `px-4`; rows `px-3.5 py-3` | Cards `gap-3`; nav sticky |
| Welcome | `px-4` | Primary inputs **`h-9`** |

**Hard rule:** never put `h-7` (or taller) pill fills inside a `--header-height` (30px) bar — they read flush against the border. Use `h-6`.

**Timeline:** gaps are **padding** (`pt-*`), never margin (virtualizer measurement).

## Audit procedure

1. **Read** `packages/desktop/DESIGN.md` (spacing canon + checklist).
2. **Scan** these hotspots for violations:
   - `components/atoms/Tab.tsx`, `TabStrip.tsx`
   - `organisms/RightPanel.tsx`, `right-panel/*`, `terminal/*`, `browser/*`
   - `organisms/SessionSidebar.tsx`, `AppHeader.tsx`, `Composer.tsx`
   - `templates/ChatShell.tsx`, `SettingsShell.tsx`
   - `pages/WelcomePage.tsx`, `pages/SettingsPage.tsx`
3. **Flag** as slop when:
   - Selected/hover fills touch a strip’s top edge or `border-b`
   - Nested chrome mixes gutters (e.g. `px-2` under a `px-2.5` TabStrip)
   - Double borders: TabStrip `border-b` + immediate child header `border-b` with no content between
   - Magic heights/paddings that ignore tokens (`text-[13px]`, bare `mt-10`, `p-6` empties)
   - Accent glow focus rings on chrome inputs (must be neutral `stroke-2`)
4. **Fix** the smallest set of shared atoms/recipes first (`Tab`/`TabStrip`/`IconButton` overrides), then propagate.
5. **Update** `DESIGN.md` if you introduce a new gutter, page, or intentional exception.
6. **Update** `COMPONENTS.md` if you add/rename components.
7. **Verify** desktop: `cd packages/desktop && pnpm test` (or `npm test -- --run`) and `npx tsc --noEmit`.

## Do / Don’t (from design-system audits)

| Do | Don’t |
|---|---|
| Reuse `Tab` / `TabStrip` / `TabClose` for panel + file chips | Copy pill markup per surface |
| Align panel body chrome to TabStrip `px-2.5` | Leave one tab header on `px-2` / `px-4` |
| Quiet sash hover (white-alpha) | Accent-colored resize lines |
| `fill-2` selected / `fill-4` hover on lists | Random `fill-3` active states |
| `--duration-fast` on chrome hover | Bare `transition-colors` |
| Keep chat mounted under settings overlays | Remount ChatPage on every settings visit |

## Report shape (when summarizing)

After an audit, report briefly:

1. **Violations found** (file + class / symptom)
2. **Fixes applied** (shared atom vs one-off)
3. **Exceptions left** (and why — document in DESIGN.md)
4. **Docs updated** (DESIGN.md / COMPONENTS.md yes/no)
