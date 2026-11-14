import { describe, expect, it } from 'vitest'
import { pinchScale, pointerDistance } from './gesture'

describe('pinchScale', () => {
  it('is the ratio of the finger distances', () => {
    expect(pinchScale(100, 200)).toBe(2)
    expect(pinchScale(200, 100)).toBe(0.5)
  })
  it('ignores a pinch that started at zero distance', () => {
    expect(pinchScale(0, 50)).toBe(1)
    expect(pinchScale(NaN, 50)).toBe(1)
  })
  it('clamps to 0.25 .. 4', () => {
    expect(pinchScale(100, 10)).toBe(0.25)
    expect(pinchScale(10, 1000)).toBe(4)
  })
})

describe('pointerDistance', () => {
  it('is the straight-line distance', () => {
    expect(pointerDistance({ x: 0, y: 0 }, { x: 3, y: 4 })).toBe(5)
  })
})
