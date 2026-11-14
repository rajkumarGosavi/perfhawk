import { describe, expect, it } from 'vitest'
import { directionMark } from './diffHelpers'

describe('directionMark', () => {
  it('marks growth and shrinkage without relying on colour', () => {
    expect(directionMark(5)).toBe('▲')
    expect(directionMark(-5)).toBe('▼')
    expect(directionMark(0)).toBe('')
  })
})
