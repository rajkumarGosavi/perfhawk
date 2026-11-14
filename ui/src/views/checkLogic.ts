export const DEFAULT_THRESHOLD = 10
export const DEFAULT_MIN_VALUE = 1

/** The terminal command that runs the same check. */
export function checkCommand(head: string, base: string, threshold: number, minValue: number, sampleType?: string): string {
  const st = sampleType ? ` --sample-type ${sampleType}` : ''
  return `perfhawk check ${head} --baseline ${base}${st} --threshold ${threshold} --min-value ${minValue}`
}

export function checkSummary(ok: boolean, regressions: number): string {
  return ok ? 'PASS' : `FAIL: ${regressions} regression${regressions === 1 ? '' : 's'}`
}

/** A number from 0 up, or null when the text is not one. */
export function parseNonNegative(text: string): number | null {
  const t = text.trim()
  if (!t) return null
  const n = Number(t)
  return Number.isFinite(n) && n >= 0 ? n : null
}

/** A focus pattern that matches exactly this function name. */
export function focusPattern(name: string): string {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return '^' + escaped + '$'
}
