import { describe, expect, it } from 'vitest'
import { themeCss } from '../testThemeCss'
import { contrastRatio } from '../contrast'
import { hashHue, labelColorFor } from './color'

const css = themeCss.replace(/\r\n/g, '\n')
const num = (re: RegExp) => Number(re.exec(css)![1])

describe('hashHue', () => {
  it('stays in the warm range and is stable', () => {
    for (const n of ['main', 'runtime.gcBgMarkWorker', '', 'x'.repeat(200)]) {
      const h = hashHue(n)
      expect(h).toBeGreaterThanOrEqual(0)
      expect(h).toBeLessThan(56)
      expect(hashHue(n)).toBe(h)
    }
  })
})

describe('labelColorFor', () => {
  const themes = [
    { name: 'light', sat: 78, light: 62 },
    { name: 'dark', sat: 60, light: 40 },
  ]
  for (const t of themes) {
    it(`gives AA contrast on every ${t.name} flame fill`, () => {
      for (let h = 0; h < 56; h++) {
        const fill = `hsl(${h} ${t.sat}% ${t.light}%)`
        expect(contrastRatio(labelColorFor(fill), fill), fill).toBeGreaterThanOrEqual(4.5)
      }
    })
  }
  it('uses the theme saturation/lightness it was tested with', () => {
    expect(num(/--flame-sat:\s*(\d+)%/)).toBe(78)
    expect(num(/--flame-light:\s*(\d+)%/)).toBe(62)
  })
  it('gives AA contrast on the search-match fill in both themes', () => {
    for (const fill of ['#1e6fe0', '#5aa7ff']) {
      expect(contrastRatio(labelColorFor(fill), fill), fill).toBeGreaterThanOrEqual(4.5)
    }
  })
  it('keeps graph node text (#111) readable across the node fill range', () => {
    for (let l = 52; l <= 92; l += 2) {
      const fill = `hsl(24 78% ${l}%)`
      expect(contrastRatio('#111111', fill), fill).toBeGreaterThanOrEqual(4.5)
    }
  })
})

