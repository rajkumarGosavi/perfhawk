# UI revamp, Phase 2 (profile workbench) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the profile page into a workbench: a timeline strip that selects the time range, a filter chip bar, and a flame graph with an inspector and a Top / Call tree / Findings panel below it.

**Architecture:** Shared chrome (timeline strip + filter bar) sits above every profile view; the Flame view becomes `Workbench` (flame + inspector + bottom tabs). All state stays in the URL (`t0`/`t1`, `focus`/`exclude`/`hide`, `tx`, new `bt`). No API changes: the strip reuses `getHeatmap`, the inspector reuses `getSandwich`.

**Tech Stack:** React 19, TypeScript, vitest + jsdom + axe-core, Radix (Popover added; Tabs, Tooltip already in), plain CSS tokens.

**Spec:** `docs/superpowers/specs/2026-11-14-ui-revamp-design.md` (Phase 2). Builds on PR 9 (Phases 0-1): branch is stacked on `feat/ui-revamp`.

## Global Constraints

- No backend/API changes. React 19 + Vite + plain CSS with tokens; Radix + cmdk only (plus `@radix-ui/react-popover@^1.3.1`, verified React 19 peer).
- `npm test` and `npm run build` pass after every task (run from `ui/`). PowerShell 5.1 syntax for user-facing commands; verify branch before each commit.
- URL-as-state; old links with `view=both|icicle|sandwich|groups|top` keep working and light Flame on the rail where `railView` says so.
- Text contrast >= 4.5:1, graphics >= 3:1 in light, dark, colourblind; severity never colour-only; new controls keyboard operable.
- Canvas flame graph stays canvas. Interaction latency: strip drag and chip edits update the URL on release, not per pointer move.

## Review Focus

- A zero-width click, reversed drag, or drag released outside the strip never writes an empty or inverted `t0`/`t1`.
- A profile without time data: no strip, no crash, `t0`/`t1` ignored as today.
- An invalid regex typed in Add filter is rejected inline and never reaches the URL or the server.
- The selected frame goes stale when filters, range or tree change: the inspector clears instead of showing old numbers.
- Function names with regex metacharacters (`List<T>.Add(int)`, `a.b*`) are escaped when Focus/Hide come from the inspector.
- Old URLs (`view=both`, `view=icicle`, `view=sandwich&func=3`) still render a sensible page.

---

## File Structure

Create: `ui/src/time/series.ts` (+test), `ui/src/views/TimelineStrip.tsx` (+test), `ui/src/views/filterModel.ts` (+test), `ui/src/views/FilterBar.tsx` (+test), `ui/src/views/Inspector.tsx`, `ui/src/views/inspectorModel.ts` (+test), `ui/src/views/Workbench.tsx`, `ui/src/styles/workbench.css`.
Modify: `ui/package.json`, `ui/src/main.tsx`, `ui/src/url.ts` (+`url.test.ts`), `ui/src/flame/FlameGraph.tsx`, `ui/src/views/ProfileView.tsx`, `ui/src/shell/views.ts` (no change expected), `ui/src/appShell.test.tsx`, `ui/src/a11y/axe.test.tsx`, `ui/src/styles/css.test.ts`, `ui/src/testThemeCss.ts`, `docs/USER_GUIDE.md`.

---

### Task 1: Branch and dependency

- [ ] **Step 1:** In the worktree `../profiling-ui-revamp`: `git switch -c feat/ui-workbench` (from `feat/ui-revamp`). Confirm `git branch --show-current`.
- [ ] **Step 2:** From `ui/`: `npm install @radix-ui/react-popover@^1.3.1`; `npm run build` and `npm test` pass.
- [ ] **Step 3:** Commit `chore(ui): add radix popover`.

### Task 2: Timeline series helpers

**Files:** Create `ui/src/time/series.ts`, `series.test.ts`.

**Interfaces (produces):**
```ts
heatToSeries(heat: Heatmap): number[]                       // length heat.cols; sum of cell values per column
xToMs(x: number, width: number, durationMs: number): number // clamped, rounded
rangeFromDrag(x0: number, x1: number, width: number, durationMs: number): Range | null // via normalizeRange; null if empty
heatDurationMs(heat: Heatmap): number                       // round(totalBuckets * bucketNs / 1e6)
```

- [ ] **Step 1: Write failing tests:** `heatToSeries` with `cols:3` and cells `[{col:0,row:0,value:2},{col:0,row:1,value:3},{col:2,row:0,value:1}]` gives `[5,0,1]`; `heatToSeries` with zero cells gives `[0,0,0]`; `xToMs(50,100,1000) === 500`, `xToMs(-5,100,1000) === 0`, `xToMs(500,100,1000) === 1000`; `rangeFromDrag(80,20,100,1000)` equals `{t0:200,t1:800}`; `rangeFromDrag(40,40,100,1000)` is `null`; `heatDurationMs({bucketNs:1e7,totalBuckets:100,...}) === 1000`.
- [ ] **Step 2:** `npx vitest run src/time/series` fails (module missing).
- [ ] **Step 3:** Implement in `series.ts`, reusing `normalizeRange` from `time/range.ts`.
- [ ] **Step 4:** Tests pass. Commit `feat(ui): timeline series helpers`.

### Task 3: TimelineStrip

**Files:** Create `ui/src/views/TimelineStrip.tsx`, `TimelineStrip.test.tsx`.

**Interfaces:** consumes `getHeatmap(ref, {st, focus, exclude, hide, tx, cols})` from `api.ts`, `toPath` from `time/sparkline.ts`, `nudge`, `formatRange`, `Range` from `time/range.ts`, helpers from Task 2.
```ts
TimelineStrip(props: {
  profileRef: string
  query: { st?: number; focus?: string; exclude?: string; hide?: string; tx?: string }
  range: Range | undefined
  onRange: (r: Range | undefined) => void
}): JSX.Element | null   // null while loading or if the heatmap is empty
```
Behaviour: header row "Time range" with a breadcrumb (`Full recording` button, then the selected range text when set; the button clears the range); an SVG sparkline of `heatToSeries(heat)` with a translucent selection band using `--accent-soft` and edge bars `--accent`; pointer drag (pointer capture) commits on release through `rangeFromDrag`; double-click and Esc clear; the SVG is `role="group"` with an `aria-label` and is focusable: ArrowLeft/Right move the range, ArrowUp/Down grow/shrink using `nudge` with a step of 2% of duration; a `role="status"` line announces the range. Layout: `.timeline-strip*` classes in `workbench.css` (Task 8).

- [ ] **Step 1: Write failing tests** (mock `getHeatmap`): renders a path after load; pointerdown at x=20, pointerup at x=80 on a 100 px wide strip calls `onRange({t0:200,t1:800})` for a 1000 ms recording; a zero-width click does not call `onRange` with a range; Escape calls `onRange(undefined)`; clicking the `Full recording` breadcrumb calls `onRange(undefined)`; ArrowRight with a range set calls `onRange` with the range shifted right; renders nothing when the heatmap has no cells.
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** Implement. jsdom has no layout, so read the width from `getBoundingClientRect()` and let the test stub it to 100.
- [ ] **Step 4:** Tests + `npm run build` pass. Commit `feat(ui): timeline strip with range selection`.

### Task 4: Filter model and FilterBar

**Files:** Create `ui/src/views/filterModel.ts`, `filterModel.test.ts`, `ui/src/views/FilterBar.tsx`, `FilterBar.test.tsx`.

**Interfaces (produces):**
```ts
type FilterKind = 'focus' | 'exclude' | 'hide'
escapeRegex(s: string): string                                     // anchors not added; escapes . * + ? ^ $ { } ( ) | [ ] \
exactRegex(name: string): string                                   // '^' + escapeRegex(name) + '$'
validRegex(s: string): boolean                                     // true if new RegExp(s) compiles and s.trim() !== ''
activeFilters(f: { focus?: string; exclude?: string; hide?: string }): { kind: FilterKind; value: string }[]  // order focus, exclude, hide; skips blank values
FilterBar(props: {
  focus?: string; exclude?: string; hide?: string; hidePh?: boolean
  onSet: (kind: FilterKind, value: string | undefined) => void
  onHidePh: (on: boolean) => void
}): JSX.Element
```
`FilterBar`: a `Chip` per active filter (label `Focus: <regex>`, remove calls `onSet(kind, undefined)`), a Perfhawk-frames toggle button (`aria-pressed`, calls `onHidePh`), and an `Add filter` button opening a Radix Popover with a kind `<select>`, a regex input (`Term` hint icons for the three kinds), an inline error `role="alert"` for invalid regex, and an Add button (Enter submits). Add on valid input calls `onSet(kind, value)` and closes; an existing kind is replaced.

- [ ] **Step 1: Write failing tests:** `escapeRegex('List<T>.Add(int)')` equals `'List<T>\\.Add\\(int\\)'`; `exactRegex('a.b*')` equals `'^a\\.b\\*$'`; `validRegex('(')` false, `validRegex('  ')` false, `validRegex('db.*')` true; `activeFilters({focus:'a',hide:'  ',exclude:'b'})` equals `[{kind:'focus',value:'a'},{kind:'exclude',value:'b'}]`. Component: chips render for set filters; remove click calls `onSet('focus', undefined)`; typing `(` and pressing Add shows an alert and does not call `onSet`; typing `db` with kind Hide and pressing Add calls `onSet('hide','db')`; toggle calls `onHidePh(true)`.
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** Implement both files.
- [ ] **Step 4:** Tests pass. Commit `feat(ui): filter chip bar with add-filter popover`.

### Task 5: Inspector model and component

**Files:** Create `ui/src/views/inspectorModel.ts`, `inspectorModel.test.ts`, `ui/src/views/Inspector.tsx`; extend `ui/src/a11y/axe.test.tsx`.

**Interfaces (produces):**
```ts
topChildren(node: FlameNode, n: number): { name: string; funcId: number | null; total: number; pct: number }[]
   // direct children by total desc; pct = child.total / node.total * 100 (0 if node.total is 0)
Inspector(props: {
  node: FlameNode | null
  rootTotal: number
  unit: string
  profileRef: string
  query: AnalysisQuery
  onTransform: (t: Transform) => void          // sub | merge | drop with the function name
  onFilter: (kind: FilterKind, value: string) => void   // Hide/Focus use exactRegex(node.name)
  onSandwich: (funcId: number) => void
}): JSX.Element
```
Behaviour: `node === null` shows a hint ("Select a frame to inspect it"). Otherwise: name (mono, wraps), file, Self and Total cards (`formatValue`, `formatPct` of `rootTotal`), buttons Focus subtree / Merge / Drop (`onTransform`), Hide (`onFilter('hide', exactRegex(name))`), Sandwich (disabled when `funcId` is null). Callers and callees lists from `getSandwich(profileRef, funcId, query)` via `topChildren(data.callers, 5)` and `topChildren(data.callees, 5)`; a loading line, and an inline error line if the request fails.

- [ ] **Step 1: Write failing tests:** `topChildren` orders by total, caps at n, computes pct, handles `total: 0`. Component (mock `getSandwich`): null node shows the hint; a node shows name, self and total; clicking Hide calls `onFilter('hide', '^List<T>\\.Add\\(int\\)$')` for that name; Merge calls `onTransform({kind:'merge',name})`; callers and callees render from the mock; Sandwich button disabled for `funcId: null`; selecting a different node refetches; a rejected request shows the error line. Add an axe case for a selected inspector.
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass. Commit `feat(ui): inspector for the selected frame`.

### Task 6: Flame selection and URL state for the bottom tabs

**Files:** Modify `ui/src/flame/FlameGraph.tsx`, `ui/src/url.ts`, `ui/src/url.test.ts`.

**Interfaces (produces):**
- `FlameGraph` gains `onSelectNode?: (n: FlameNode) => void` and `selected?: FlameNode | null`; a click (already zooms) also calls `onSelectNode(node)`; the selected node is drawn with a 2 px `--match` outline (resolved like existing colours). Keyboard: Enter (already zooms) also selects.
- `ViewState` gains `bt?: 'tree' | 'findings'` (bottom tab; `top` is the default and omitted from the URL); written as `bt`.

- [ ] **Step 1: Write failing tests** in `url.test.ts`: `bt=tree` and `bt=findings` round-trip; `bt=top`, empty, and unknown values read as undefined; `writeViewState` omits an undefined `bt`.
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** Implement `bt` in `url.ts`; add the two props to `FlameGraph` (no unit test: the component draws on canvas, which jsdom cannot; verified in Task 9's browser check).
- [ ] **Step 4:** `npm test`, `npm run build`. Commit `feat(ui): flame selection callback and bt url state`.

### Task 7: Workbench component

**Files:** Create `ui/src/views/Workbench.tsx`; modify `ui/src/appShell.test.tsx`.

**Interfaces:** consumes `FlameGraph`, `Inspector`, `TopTable`, `CallTree`, `Findings`, `Tabs` (from `ui/Tabs`), `FlameNode`, `TopRow`.
```ts
Workbench(props: {
  tree: FlameNode; rows: TopRow[]; unit: string
  inverted: boolean
  profileRef: string
  query: AnalysisQuery
  search?: string; highlight?: ReadonlySet<FlameNode>
  exportRef: { current: FlameExportApi | null }
  bottomTab: 'top' | 'tree' | 'findings'
  onBottomTab: (t: 'top' | 'tree' | 'findings') => void
  sort: 'self' | 'total'; onSort: (s: 'self' | 'total') => void
  hot: boolean
  onTransform: (t: Transform) => void
  onFilter: (kind: FilterKind, value: string) => void
  onSandwich: (funcId: number) => void
  navigate: Nav
}): JSX.Element
```
Layout: row with `FlameGraph` (flex 1) and `Inspector` aside; below, `Tabs` labelled "Tables" with Top functions, Call tree, Findings. The selected node is local state and is cleared when `tree` changes (new object) so stale numbers never show. Selecting a Top row selects nothing in the flame but opens the Sandwich (existing behaviour, `onSandwich`).

- [ ] **Step 1: Write failing tests** (extend `appShell.test.tsx`, fetch stubbed as there, add a flame tree with one child `db.Query` and a top row): `?profile=p1&view=both` renders an element labelled "Inspector" and a tablist labelled "Tables" with three tabs; clicking the Call tree tab writes `bt=tree` to the URL; the inspector shows the hint before any selection.
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** Implement `Workbench`.
- [ ] **Step 4:** Tests pass. Commit `feat(ui): workbench layout component`.

### Task 8: Wire shared chrome into ProfileView, styles, docs

**Files:** Modify `ui/src/views/ProfileView.tsx`, `ui/src/main.tsx`, `ui/src/styles/css.test.ts`, `ui/src/testThemeCss.ts`, `ui/src/appShell.test.tsx`, `docs/USER_GUIDE.md`; create `ui/src/styles/workbench.css`.

- [ ] **Step 1: Write failing tests** (`appShell.test.tsx`): for a profile with time data and `view=flame`, a "Time range" section exists; for one without time data it does not; the Focus/Exclude/Hide text inputs are gone and an `Add filter` button exists; `?profile=p1&view=flame&focus=db` shows a `Focus: db` chip and its remove button writes a URL without `focus`; `view=both`, `view=icicle` and `view=flame` all render the inspector; the Flame-layout switch has only Flame and Icicle. Also assert `css.test.ts` covers `workbench.css` (token-only colours, declared vars).
- [ ] **Step 2:** Run; FAIL.
- [ ] **Step 3:** In `ProfileView`: for `view` in flame, icicle, both load flame tree + top rows (the existing `both` data) and render `Workbench` (`inverted = view === 'icicle'`); delete the separate flame-only and both-only blocks; `FLAME_LAYOUTS` becomes Flame and Icicle. Replace the Focus/Exclude/Hide toolbar and the "Window" chip with `FilterBar` (committing through `replace({ [kind]: value })`, `hidePh` through `navigate`) and, above every view except summary, time, diff, `TimelineStrip` when `record.has_timeline` (range from `t0`/`t1`, `onRange` through `replace`). Keep the Baseline chip and "Diff window vs baseline" button. Remove `useDebounced` for focus/exclude/hide (keep it for search) and the filters disclosure state. Keep `activeFilterCount`/`filtersToggleLabel` in `filters.ts` only if still imported; otherwise delete them and their tests. Add `workbench.css` (strip, filter bar, workbench grid, inspector, tables panel; `@media (max-width: 900px)` stacks the inspector under the flame) and import it in `main.tsx`; add it to `testThemeCss.ts` and `css.test.ts`.
- [ ] **Step 4:** Update `docs/USER_GUIDE.md` (toolbar section: timeline strip, filter chips, inspector, Tables tabs; `view=both` and `view=icicle` links still work).
- [ ] **Step 5:** `npm test`, `npm run lint`, `npm run build` pass. Commit `feat(ui): profile workbench with timeline strip and filter chips`.

### Task 9: Verification

- [ ] **Step 1:** Add axe cases to `a11y/axe.test.tsx` for `FilterBar` and `TimelineStrip` (mock `getHeatmap`); run the whole suite.
- [ ] **Step 2: Manual check** (real server as in Phases 0-1: import a profile with time data such as a `perfhawk record` capture, run `perfhawk ui`, `npm run dev -- --port 5199`): drag a range on the strip and watch the flame graph, tables and URL update; Esc and the breadcrumb clear it; add a Focus filter through the popover, reject `(`; click a frame and see the inspector fill, then Hide from the inspector; switch the Tables tabs; resize to 375 px and confirm no horizontal page scroll and the inspector stacks; light, dark and colourblind palette by eye. Record results in the PR description.
- [ ] **Step 3:** Push `feat/ui-workbench`, open a PR against `feat/ui-revamp` (stacked on PR 9). Ask before opening.

---

## Self-review

- **Spec coverage (Phase 2):** timeline strip with range selection and breadcrumbs -> Tasks 2-3, 8; filter chip bar -> Task 4, 8; flame + inspector split -> Tasks 5-7; bottom tabs Top / Call tree / Findings -> Tasks 6-7; Both retired as a view (alias), Icicle becomes a toggle, Sandwich via inspector -> Tasks 7-8. Deferred: "Show in timeline" for a function (needs a per-function time series the API does not offer), Popover for glossary terms, key-map revision (Top/Call tree/Groups stay full-page views with the shared chrome).
- **Type consistency:** `FilterKind`/`exactRegex` (Task 4) are used by Inspector (Task 5) and Workbench (Task 7); `bt` (Task 6) feeds `bottomTab` (Task 7); `Range` helpers (Task 2) feed the strip (Task 3).
- **Proportion:** decisions and test cases, no bodies; the canvas selection change has no unit test and is covered by the browser check.
