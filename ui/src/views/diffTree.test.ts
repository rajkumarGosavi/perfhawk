import { describe, expect, it } from 'vitest'
import type { DiffNode } from '../api'
import { layoutFlame } from '../flame/layout'
import { toFlameNodes } from './diffTree'

const d = (name: string, base: number, head: number, children: DiffNode[] = []): DiffNode => ({
  name, file: '', baseSelf: 0, baseTotal: base, headSelf: 0, headTotal: head, children,
})

describe('toFlameNodes', () => {
  // Cost moved between callees: X shrank 10 -> 0 while Y grew 0 -> 10 under an unchanged parent.
  const root = d('root', 10, 10, [d('A', 10, 10, [d('X', 10, 0), d('Y', 0, 10)])])

  it('a parent is at least as wide as its children together', () => {
    const t = toFlameNodes(root)
    const a = t.children[0]
    expect(a.total).toBe(20)
    expect(t.total).toBe(20)
    expect(a.children.map((c) => c.total)).toEqual([10, 10])
  })

  it('children never overlap or leave their parent in the layout', () => {
    const rects = layoutFlame(toFlameNodes(root), 1000, 18, false)
    const by = (n: string) => rects.find((r) => r.node.name === n)!
    const x = by('X'), y = by('Y'), a = by('A')
    expect(x.x + x.w).toBeLessThanOrEqual(y.x + 1e-9)
    expect(y.x + y.w).toBeLessThanOrEqual(a.x + a.w + 1e-9)
  })

  it('keeps base/head values for colouring and handles very deep trees', () => {
    const t = toFlameNodes(root)
    expect(t.children[0].children[0]).toMatchObject({ base: 10, head: 0 })
    let deep = d('leaf', 1, 2)
    for (let i = 0; i < 20000; i++) deep = d(`f${i}`, 1, 2, [deep])
    expect(() => toFlameNodes(deep)).not.toThrow()
  })
})
