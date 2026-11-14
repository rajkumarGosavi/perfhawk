import { describe, expect, it } from 'vitest'
import { activeFilterCount } from './filters'

describe('activeFilterCount', () => {
  it('is zero when nothing is set', () => {
    expect(activeFilterCount({})).toBe(0)
    expect(activeFilterCount({ focus: '', exclude: '', hide: '' })).toBe(0)
  })
  it('counts each active filter once', () => {
    expect(activeFilterCount({ focus: 'main', hidePh: true })).toBe(2)
    expect(activeFilterCount({ focus: 'a', exclude: 'b', hide: 'c', hidePh: true })).toBe(4)
  })
  it('ignores whitespace-only text', () => {
    expect(activeFilterCount({ focus: '   ' })).toBe(0)
  })
  it('does not count an unset hidePh', () => {
    expect(activeFilterCount({ hidePh: undefined })).toBe(0)
  })
})
