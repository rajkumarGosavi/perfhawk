export type FilterKind = 'focus' | 'exclude' | 'hide'

export const FILTER_LABEL: Record<FilterKind, string> = { focus: 'Focus', exclude: 'Exclude', hide: 'Hide' }

/** Escapes regex metacharacters so a function name matches literally. */
export function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** A regex matching exactly this function name. */
export function exactRegex(name: string): string {
  return `^${escapeRegex(name)}$`
}

/** True when the text compiles as a regular expression and is not blank. */
export function validRegex(s: string): boolean {
  if (s.trim() === '') return false
  try {
    new RegExp(s)
    return true
  } catch {
    return false
  }
}

/** The filters that are set, in a stable order; blank values are skipped. */
export function activeFilters(f: { focus?: string; exclude?: string; hide?: string }): { kind: FilterKind; value: string }[] {
  const out: { kind: FilterKind; value: string }[] = []
  for (const kind of ['focus', 'exclude', 'hide'] as const) {
    const value = f[kind]
    if (value !== undefined && value.trim() !== '') out.push({ kind, value })
  }
  return out
}
