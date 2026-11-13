import { describe, expect, it } from 'vitest'
import type { DiffNode, DiffRow } from '../api'
import type { DiffFlameNode } from './diffTree'
import { diffStatus, filterDiffTree, filterRows, toSideTree } from './diffHelpers'

const d = (name: string, base: number, head: number, children: DiffNode[] = []): DiffNode => ({
  name, file: '', baseSelf: base, baseTotal: base, headSelf: head, headTotal: head, children,
})
const row = (name: string, deltaSelf: number): DiffRow => ({
  name, file: '', baseSelf: 0, headSelf: 0, deltaSelf, baseTotal: 0, headTotal: 0, deltaTotal: deltaSelf,
})

describe('diffStatus', () => {
  it('classifies', () => {
    expect(diffStatus(0, 5)).toBe('new')
    expect(diffStatus(5, 0)).toBe('gone')
    expect(diffStatus(5, 5)).toBe('same')
    expect(diffStatus(5, 6)).toBe('changed')
    expect(diffStatus(0, 0)).toBe('same')
  })
})

describe('filterRows', () => {
  const rows = [row('up', 3), row('down', -2), row('flat', 0)]
  it('filters by sign of deltaSelf', () => {
    expect(filterRows(rows, 'regressions').map((r) => r.name)).toEqual(['up'])
    expect(filterRows(rows, 'improvements').map((r) => r.name)).toEqual(['down'])
    expect(filterRows(rows, undefined)).toBe(rows)
    expect(filterRows([], 'regressions')).toEqual([])
  })
})

describe('filterDiffTree', () => {
  const root = d('root', 10, 12, [d('grow', 5, 8, [d('leaf', 1, 4)]), d('shrink', 5, 2), d('same', 3, 3)])
  it('keeps only growing subtrees for regressions', () => {
    const t = filterDiffTree(root, 'regressions')
    expect(t.children.map((c) => c.name)).toEqual(['grow'])
    expect(t.children[0].children.map((c) => c.name)).toEqual(['leaf'])
  })
  it('keeps shrinking for improvements and the root when nothing matches', () => {
    expect(filterDiffTree(root, 'improvements').children.map((c) => c.name)).toEqual(['shrink'])
    const t = filterDiffTree(d('r', 1, 1, [d('x', 1, 1)]), 'regressions')
    expect(t.name).toBe('r')
    expect(t.children).toEqual([])
    expect(filterDiffTree(root, undefined)).toBe(root)
  })
  it('handles a 10 000-deep chain', () => {
    let n = d('leaf', 1, 2)
    for (let i = 0; i < 10_000; i++) n = d('n' + i, 1, 2, [n])
    let depth = 0
    for (let c: DiffNode | undefined = filterDiffTree(n, 'regressions'); c; c = c.children[0]) depth++
    expect(depth).toBe(10_001)
  })
})

describe('toSideTree', () => {
  const root = d('root', 10, 10, [d('onlyBase', 4, 0), d('onlyHead', 0, 6), d('both', 6, 4)])
  it('omits nodes absent on that side and keeps both values', () => {
    const b = toSideTree(root, 'base')
    expect(b.children.map((c) => c.name)).toEqual(['onlyBase', 'both'])
    expect(b.children[1].total).toBe(6)
    expect((b.children[1] as DiffFlameNode).base).toBe(6)
    expect((b.children[1] as DiffFlameNode).head).toBe(4)
    const h = toSideTree(root, 'head')
    expect(h.children.map((c) => c.name)).toEqual(['onlyHead', 'both'])
    expect(h.children[1].total).toBe(4)
  })
  it('identical profiles give equal sides; empty root is childless', () => {
    const same = d('r', 5, 5, [d('a', 5, 5)])
    expect(toSideTree(same, 'base').children.map((c) => c.total)).toEqual(
      toSideTree(same, 'head').children.map((c) => c.total))
    const e = toSideTree(d('r', 0, 0), 'base')
    expect(e.children).toEqual([])
    expect(e.total).toBe(0)
  })
})
