import { describe, expect, it } from 'vitest'
import { nextFocusIndex } from './focus'

describe('nextFocusIndex', () => {
  it('wraps forward past the last item', () => {
    expect(nextFocusIndex(3, 2, false)).toBe(0)
  })
  it('wraps backward past the first item', () => {
    expect(nextFocusIndex(3, 0, true)).toBe(2)
  })
  it('moves one step in the middle', () => {
    expect(nextFocusIndex(3, 1, false)).toBe(2)
    expect(nextFocusIndex(3, 1, true)).toBe(0)
  })
  it('stays on the only item', () => {
    expect(nextFocusIndex(1, 0, false)).toBe(0)
    expect(nextFocusIndex(1, 0, true)).toBe(0)
  })
  it('returns -1 when there is nothing to focus', () => {
    expect(nextFocusIndex(0, 0, false)).toBe(-1)
  })
  it('starts at the first (or last) item when nothing is focused yet', () => {
    expect(nextFocusIndex(3, -1, false)).toBe(0)
    expect(nextFocusIndex(3, -1, true)).toBe(2)
  })
})
