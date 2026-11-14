type Filters = { focus?: string; exclude?: string; hide?: string; hidePh?: boolean }

/** How many filters inside the "Filters" disclosure are active (Search only highlights, so it is not counted). */
export function activeFilterCount(f: Filters): number {
  const text = [f.focus, f.exclude, f.hide].filter((v) => (v ?? '').trim() !== '').length
  return text + (f.hidePh ? 1 : 0)
}

/** Button text that says what a click will do, and how many filters are hiding while collapsed. */
export function filtersToggleLabel(open: boolean, activeCount: number): string {
  if (open) return 'Hide filters'
  return activeCount > 0 ? `Show filters (${activeCount} active)` : 'Show filters'
}
