# Perfhawk UI revamp: workbench design

Status: draft, awaiting approval. Mockup: https://claude.ai/artifact/4rMuT6VU3FV7ftnP3S3o34 (private).

## Intent

Turn the browser UI from a row of ten flat tabs into a profiler workbench: one shared time range, one shared filter set, and linked views. Users should go from "something is slow" to "this function, called from here" in a few clicks, without re-learning the toolbar per view.

## Goals

- One shell: top bar (breadcrumb, command palette, export), left rail (views grouped Analyze / Compare), content area.
- Shared state across views: time range (`t0`/`t1`), filters (focus/exclude/hide/search), selected function. Already URL-backed; make it visible and editable in the UI.
- Timeline strip above the analysis, drag to select, zoom breadcrumbs. Flame graph, tables and inspector follow the range.
- Function actions (Focus, Sandwich, Hide, Show in timeline) live in an inspector and context menu, not as separate tabs.
- Findings-first Summary: lead with the top finding as a sentence, then numbers, ranked findings, hot path.
- Command palette (Ctrl K): jump to view, search functions, run export/compare.
- A real design system: primitive and semantic tokens, light and dark, colourblind palette, density.
- Keep or improve accessibility. Axe tests stay green, severity never colour-only, chart colours pass 3:1 on their surface in both themes.

## Non-goals

- No backend/API changes. No new analysis features.
- No framework swap, no styled UI kit, no Tailwind. React 19 + Vite + plain CSS with tokens, plus headless Radix primitives and cmdk (see Owner decisions).
- No AI features (parked, see memory).
- No mobile-first redesign. Must work at phone width, but the workbench is desktop-first.

## Anti-goals

- A restyle that leaves the ten-tab structure intact.
- Losing URL-as-state, deep links, keyboard shortcuts, or screen-reader support.
- Slower flame graph. It stays canvas.

## Constraints

- Existing code: `ui/src/App.tsx` (no shell), single `theme.css` (328 lines), views in `ui/src/views/`, keys in `ui/src/analysis/keys.ts` (1-9 map to views), state in `ui/src/url.ts`.
- UI is embedded into the Go binary at build; verify branch before committing (see memory).
- Tests: vitest + axe in `ui/src/a11y/axe.test.tsx`. Must keep passing each phase.

## Approaches considered

1. **Restyle only.** New tokens and CSS, same layout. Cheapest. Fails the goal: structure is the main problem.
2. **Workbench shell, migrate views one at a time (recommended).** Build shell and shared state first, then move views in. Each phase ships and passes tests. Old tabs keep working until replaced.
3. **Big-bang rewrite on a branch.** Cleanest end state, but long-lived branch, large review, and the GC work on this branch is unmerged. Rejected.

Chosen: approach 2. Reason: shippable increments, lower risk, reviewable PRs.

## Research basis

- Firefox Profiler / Perfetto: timeline on top, range selection narrows the aggregate views.
- Chrome DevTools: zoom breadcrumbs, hide-frame controls.
- Pyroscope / Grafana: table + flame side by side, sandwich from a row or context menu.
- Profile Explorer: cross-view highlight of a function's samples.
- Dev-tool UX: dense but clean, command palette, sub-200 ms feedback, progressive disclosure.
- Chart accessibility: re-derive dark series colours, check 3:1, never red/green alone.

## Design decisions

- Layout: see mockup artboards "Workbench" and "Summary".
- View map: Summary(1), Flame(2), Call tree(3), Timeline(4), Call graph(5), GC(6), Diff(7), History(8). Icicle becomes a Flame toggle. Both becomes the default flame + tables layout. Sandwich becomes an inspector action. Groups becomes a table option under Top. Key map changes accordingly (breaking, documented in Help).
- Tokens: primitive (palette, space, radius, type) then semantic (surface, text, border, accent, danger, flame heat ramp, diff ramp). Components only use semantic tokens.
- Type: Inter for UI, JetBrains Mono for code/numbers, system fallbacks. Fonts bundled, no network. (Mockup used Google Fonts for preview only.)
- Accent: amber dark, blue light. Final colours are a builder call, verified by a contrast script.

## Phases (each its own PR, tests green)

0. **Foundations.** (Prerequisite: GC branch merged.) Split `theme.css` into tokens/base/components. Add Radix + cmdk deps. Add primitives: Button, IconButton, Chip, Tabs, Table, Panel (Radix Dialog), Menu (Radix Dropdown), Tooltip, Popover. Contrast checker script for token pairs in both themes. No visual change except tokens.
1. **App shell.** Top bar, left rail, breadcrumb, responsive collapse. Command palette. New key map. Help updated.
2. **Profile workbench.** Timeline strip with range selection and breadcrumbs (reuse `time/range.ts`, `heatLayout.ts`). Filter chip bar. Flame + inspector split. Bottom tabs Top / Call tree / Findings. Retire Both, Icicle, Sandwich tabs.
3. **Summary, Home, History.** Findings-first Summary, Home with recent profiles and capture, History chart in the new style.
4. **Remaining views.** Diff, GC, Call graph, Timeline heat map moved into the shell and restyled; GC section loses its ad-hoc CSS.
5. **Polish.** Responsive pass, density option, motion, empty/error/loading states, full axe + keyboard audit, flame graph performance check, docs (`USER_GUIDE.md`, `VISUALIZATION.md`, `A11Y_CHECKLIST.md`) and screenshots.

## Verification

- `npm test` and axe suite each phase. Contrast script in CI.
- Manual keyboard walk-through of shell, palette, timeline, flame, inspector.
- Render check at 1440, 1024, 390 px, both themes, colourblind palette.

## Left to the builder

- Exact accent and ramp values (must pass the contrast script).
- Rail icons, palette fuzzy-match implementation.
- Whether timeline range drag reuses heat-map code or a new canvas.
- Component API details.

## Owner decisions (2026-11-14)

- Main user: mid-level and senior backend engineers working on an application, plus perf engineers. So: dense, keyboard-first workbench. Guidance stays in Summary, Help and tooltips, not in the main layout.
- The 1-9 key map may break.
- Merge the GC branch (`feat/gc-analysis`) before Phase 0, so the revamp does not restyle unmerged code.
- UI library: Radix primitives plus cmdk, headless, styled by our tokens. Add only what is used: `@radix-ui/react-dialog`, `-tabs`, `-dropdown-menu`, `-popover`, `-tooltip`, and `cmdk`. All verified to support React 19. Replaces hand-rolled Panel, ExportMenu, Term popover, tab bars and the palette. Everything else (flame canvas, timeline, tables, charts) stays ours. Rejected: React Aria Components and Base UI (much larger, no palette), no library (focus and keyboard bugs).

## Amendments (from the Phase 0-1 plan, 2026-11-14)

- The `Term` popover stays hand-rolled: it already meets WCAG 1.4.13 and is tested. `@radix-ui/react-popover` is deferred to the Phase 2 plan (filter-add popover).
- Key map for Phases 0-1: `1` Summary, `2` Flame, `3` Call tree, `4` Top, `5` Timeline, `6` Call graph, `7` Groups, `8` History. GC is a session-level section on Home (not a profile view) until Phase 4, so it is not on the rail yet. Phase 2 revises the map when Top and Groups fold into the flame view.
- The GC branch is already merged to main (PR #7), so the Phase 0 prerequisite holds.
