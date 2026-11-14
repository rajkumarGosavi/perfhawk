import { describe, expect, it } from 'vitest'
import { activeFilterCount, filtersToggleLabel } from './filters'

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

describe('filtersToggleLabel', () => {
  it('says what a click will do', () => {
    expect(filtersToggleLabel(true, 0)).toBe('Hide filters')
    expect(filtersToggleLabel(false, 0)).toBe('Show filters')
  })
  it('shows how many filters are active while collapsed', () => {
    expect(filtersToggleLabel(false, 2)).toBe('Show filters (2 active)')
    expect(filtersToggleLabel(true, 2)).toBe('Hide filters')
  })
})
