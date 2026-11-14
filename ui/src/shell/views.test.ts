import { describe, expect, it } from 'vitest'
import { RAIL, railView } from './views'

describe('RAIL', () => {
  it('lists the views in key order with unique keys', () => {
    expect(RAIL.map((r) => [r.key, r.view])).toEqual([
      ['1', 'summary'], ['2', 'flame'], ['3', 'tree'], ['4', 'top'], ['5', 'time'], ['6', 'graph'], ['7', 'groups'], ['8', 'history'],
    ])
    expect(new Set(RAIL.map((r) => r.key)).size).toBe(RAIL.length)
  })
  it('only Timeline needs time data; History is in the compare group', () => {
    expect(RAIL.filter((r) => r.needsTimeline).map((r) => r.view)).toEqual(['time'])
    expect(RAIL.find((r) => r.view === 'history')?.group).toBe('compare')
  })
})

describe('railView', () => {
  it('folds legacy views onto the flame item and leaves others alone', () => {
    expect(railView('icicle')).toBe('flame')
    expect(railView('both')).toBe('flame')
    expect(railView('sandwich')).toBe('flame')
    expect(railView('diff')).toBe('diff')
    expect(railView('tree')).toBe('tree')
  })
})
