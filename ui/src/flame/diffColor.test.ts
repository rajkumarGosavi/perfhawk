import { describe, expect, it } from 'vitest'
import { diffColor } from './diffColor'

describe('diffColor', () => {
  it('is flat when nothing changed', () => {
    expect(diffColor(10, 10)).toBe('var(--diff-flat)')
    expect(diffColor(0, 0)).toBe('var(--diff-flat)')
  })
  it('growth is an up bucket, shrink a down bucket', () => {
    expect(diffColor(10, 20)).toMatch(/^var\(--diff-up-[0-4]\)$/)
    expect(diffColor(20, 10)).toMatch(/^var\(--diff-down-[0-4]\)$/)
  })
  it('appearing and vanishing frames use the strongest bucket', () => {
    expect(diffColor(0, 5)).toBe('var(--diff-up-4)')
    expect(diffColor(5, 0)).toBe('var(--diff-down-4)')
  })
  it('stronger change gives a higher bucket', () => {
    const n = (s: string) => Number(s.match(/(\d)\)$/)![1])
    expect(n(diffColor(100, 101))).toBeLessThan(n(diffColor(100, 300)))
    expect(n(diffColor(100, 300))).toBeLessThanOrEqual(4)
  })
})
