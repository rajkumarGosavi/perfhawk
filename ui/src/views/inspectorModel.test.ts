import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { topChildren } from './inspectorModel'

const n = (name: string, total: number, children: FlameNode[] = []): FlameNode => ({ funcId: total, name, file: '', selfValue: 0, total, children })

describe('topChildren', () => {
  it('orders by total, caps at n and computes the share', () => {
    const r = topChildren(n('root', 100, [n('a', 10), n('b', 60), n('c', 30)]), 2)
    expect(r.map((x) => [x.name, x.pct])).toEqual([['b', 60], ['c', 30]])
  })
  it('does not divide by zero', () => {
    expect(topChildren(n('root', 0, [n('a', 0)]), 5)[0].pct).toBe(0)
  })
  it('is empty for a leaf or n = 0', () => {
    expect(topChildren(n('leaf', 5), 5)).toEqual([])
    expect(topChildren(n('root', 5, [n('a', 5)]), 0)).toEqual([])
  })
  it('does not reorder the input', () => {
    const root = n('root', 100, [n('a', 10), n('b', 60)])
    topChildren(root, 5)
    expect(root.children.map((c) => c.name)).toEqual(['a', 'b'])
  })
})
