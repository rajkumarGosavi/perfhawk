import { describe, expect, it } from 'vitest'
import type { CallGraph, GraphNode } from '../api'
import type { Placed } from './layout'
import { edgeWidth, heat, moveFocus, neighbours, nodeSize } from './model'

const node = (funcId: number, name: string, self: number, total: number): GraphNode => ({ funcId, name, file: '', selfValue: self, total })
// diamond: 0 -> 1 (6), 0 -> 2 (4), 1 -> 3 (6), 2 -> 3 (4)
const g: CallGraph = {
  nodes: [node(0, 'a', 0, 10), node(1, 'b', 0, 6), node(2, 'c', 0, 4), node(3, 'd', 10, 10)],
  edges: [
    { from: 0, to: 1, value: 6, elided: false },
    { from: 0, to: 2, value: 4, elided: false },
    { from: 1, to: 3, value: 6, elided: false },
    { from: 2, to: 3, value: 4, elided: true },
  ],
  hiddenNodes: 0,
  totalValue: 10,
}
const placed: Placed[] = [
  { id: 0, x: 50, y: 0, layer: 0 },
  { id: 1, x: 0, y: 100, layer: 1 },
  { id: 2, x: 150, y: 100, layer: 1 },
  { id: 3, x: 50, y: 200, layer: 2 },
]

describe('sizes and weights', () => {
  it('nodeSize stays within bounds and is finite for a zero total', () => {
    for (const total of [0, 10, 1e12]) {
      const s = nodeSize(node(1, 'x'.repeat(500), 5, 5), total)
      expect(s.w).toBeGreaterThanOrEqual(80)
      expect(s.w).toBeLessThanOrEqual(260)
      expect(s.h).toBeGreaterThanOrEqual(24)
      expect(s.h).toBeLessThanOrEqual(64)
    }
    expect(nodeSize(node(1, 'a', 1, 1), 100).w).toBe(80)
  })
  it('heat is 0..1 and 0 for a zero total', () => {
    expect(heat(node(1, 'a', 5, 5), 10)).toBe(0.5)
    expect(heat(node(1, 'a', 5, 5), 0)).toBe(0)
    expect(heat(node(1, 'a', 50, 50), 10)).toBe(1)
  })
  it('edgeWidth is 1..8 and 1 for a zero max', () => {
    expect(edgeWidth(5, 0)).toBe(1)
    expect(edgeWidth(0, 10)).toBe(1)
    expect(edgeWidth(10, 10)).toBe(8)
    expect(edgeWidth(5, 10)).toBeGreaterThan(1)
  })
})

describe('neighbours', () => {
  it('orders callers and callees by edge value', () => {
    expect(neighbours(g, 0)).toEqual({ callers: [], callees: [1, 2] })
    expect(neighbours(g, 3)).toEqual({ callers: [1, 2], callees: [] })
  })
})

describe('moveFocus', () => {
  it('null selects the largest total', () => {
    expect(moveFocus(g, placed, null, 'down')).toBe(0)
  })
  it('down goes to the heaviest callee, up to the heaviest caller', () => {
    expect(moveFocus(g, placed, 0, 'down')).toBe(1)
    expect(moveFocus(g, placed, 3, 'up')).toBe(1)
  })
  it('left and right move within the layer by x', () => {
    expect(moveFocus(g, placed, 1, 'right')).toBe(2)
    expect(moveFocus(g, placed, 2, 'left')).toBe(1)
  })
  it('stays put at the boundary', () => {
    expect(moveFocus(g, placed, 0, 'up')).toBe(0)
    expect(moveFocus(g, placed, 3, 'down')).toBe(3)
    expect(moveFocus(g, placed, 1, 'left')).toBe(1)
    expect(moveFocus(g, placed, 2, 'right')).toBe(2)
  })
  it('empty graph gives null', () => {
    expect(moveFocus({ nodes: [], edges: [], hiddenNodes: 0, totalValue: 0 }, [], null, 'down')).toBeNull()
  })
})
