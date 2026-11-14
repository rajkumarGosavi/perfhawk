import { describe, expect, it } from 'vitest'
import { viewTitle } from './title'

describe('viewTitle', () => {
  it('is just the app name on Home', () => {
    expect(viewTitle({ view: 'summary' })).toBe('Perfhawk')
  })
  it('names the history page', () => {
    expect(viewTitle({ view: 'history' })).toBe('History · Perfhawk')
  })
  it('names the view and the profile', () => {
    expect(viewTitle({ view: 'flame', profile: 'abc123' }, 'myapp.pprof')).toBe('Flame graph · myapp.pprof · Perfhawk')
  })
  it('falls back to the profile reference until its name has loaded', () => {
    expect(viewTitle({ view: 'top', profile: 'abc123' })).toBe('Top functions · abc123 · Perfhawk')
  })
  it('names diffs with both sides when known', () => {
    expect(viewTitle({ view: 'diff', profile: 'head1', base: 'base1' }, 'head.pprof')).toBe('Diff · head.pprof · Perfhawk')
  })
  it('has a title for every view', () => {
    for (const v of ['summary', 'flame', 'icicle', 'tree', 'top', 'both', 'sandwich', 'diff', 'groups', 'graph', 'time', 'history'] as const) {
      expect(viewTitle({ view: v, profile: 'p' }, 'n')).toContain('Perfhawk')
    }
  })
})
