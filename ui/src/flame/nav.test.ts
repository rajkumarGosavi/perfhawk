import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { describeFrame, navigate } from './nav'

const n = (name: string, selfValue: number, children: FlameNode[] = []): FlameNode => ({
  funcId: null, name, file: '', selfValue,
  total: selfValue + children.reduce((s, c) => s + c.total, 0), children,
})

const leafA = n('leafA', 5)
const leafB = n('leafB', 20)
const mid = n('mid', 1, [leafA, leafB])
const other = n('other', 3)
const root = n('root', 0, [mid, other])

describe('navigate', () => {
  it('stays on the root when asked for its parent', () => {
    expect(navigate(root, root, 'parent')).toBe(root)
  })
  it('moves to the parent', () => {
    expect(navigate(root, leafA, 'parent')).toBe(mid)
  })
  it('child picks the child with the biggest total', () => {
    expect(navigate(root, mid, 'child')).toBe(leafB)
  })
  it('a leaf has no child to move to', () => {
    expect(navigate(root, other, 'child')).toBe(other)
  })
  it('moves between siblings and stops at the ends', () => {
    expect(navigate(root, mid, 'next')).toBe(other)
    expect(navigate(root, other, 'next')).toBe(other)
    expect(navigate(root, other, 'prev')).toBe(mid)
    expect(navigate(root, mid, 'prev')).toBe(mid)
  })
  it('stays put for a node outside the tree', () => {
    const stranger = n('stranger', 1)
    expect(navigate(root, stranger, 'parent')).toBe(stranger)
  })
  it('never moves above the given root (zoomed subtree)', () => {
    expect(navigate(mid, mid, 'parent')).toBe(mid)
    expect(navigate(mid, leafA, 'parent')).toBe(mid)
  })
  it('skips nodes that are not drawn (too narrow)', () => {
    const visible = new Set<FlameNode>([root, mid, other, leafA]) // leafB is not drawn
    expect(navigate(root, mid, 'child', visible)).toBe(leafA)
    expect(navigate(root, leafA, 'next', visible)).toBe(leafA)
  })
  it('works on a single-node tree', () => {
    const solo = n('solo', 4)
    for (const d of ['parent', 'child', 'prev', 'next'] as const) expect(navigate(solo, solo, d)).toBe(solo)
  })
})

describe('describeFrame', () => {
  it('names the frame with self, total and share of the root', () => {
    const s = describeFrame(mid, root, 'samples')
    expect(s).toContain('mid')
    expect(s).toMatch(/self/i)
    expect(s).toMatch(/total/i)
    expect(s).toContain('%')
  })
  it('does not produce NaN for an empty root', () => {
    expect(describeFrame(n('z', 0), n('z', 0), 'samples')).not.toContain('NaN')
  })
})
