import { describe, expect, it } from 'vitest'
import { themeCss } from './testThemeCss'
import { contrastRatio } from './contrast'

/** Custom properties declared in the first rule whose selector is exactly `selector`. */
function vars(selector: string): Record<string, string> {
  const css = themeCss.replace(/\r\n/g, '\n')
  const start = css.indexOf(`${selector} {`)
  if (start < 0) throw new Error(`no rule for ${selector}`)
  const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('\n}', start))
  const out: Record<string, string> = {}
  for (const m of body.matchAll(/(--[\w-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim()
  return out
}

describe('contrastRatio', () => {
  it('is 21 for black on white and 1 for identical colours', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 0)
    expect(contrastRatio('#336699', '#336699')).toBeCloseTo(1, 5)
  })
  it('accepts shorthand hex and hsl()', () => {
    expect(contrastRatio('#000', '#fff')).toBeCloseTo(21, 0)
    expect(contrastRatio('hsl(0 0% 0%)', 'hsl(0 0% 100%)')).toBeCloseTo(21, 0)
  })
})

describe('theme tokens meet WCAG AA text contrast (4.5:1)', () => {
  const light = vars(':root')
  const dark = { ...light, ...vars(":root[data-theme='dark']") }
  const pairs: [string, string][] = [
    ['--fg', '--bg'], ['--fg', '--surface'], ['--muted', '--bg'], ['--muted', '--surface'],
    ['--accent', '--bg'], ['--accent', '--surface'], ['--accent-fg', '--accent'], ['--danger-fg', '--danger-bg'],
  ]
  for (const [name, theme] of [['light', light], ['dark', dark]] as const) {
    for (const [fg, bg] of pairs) {
      it(`${name}: ${fg} on ${bg}`, () => {
        expect(contrastRatio(theme[fg], theme[bg])).toBeGreaterThanOrEqual(4.5)
      })
    }
  }
})
