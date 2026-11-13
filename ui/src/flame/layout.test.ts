import { describe, expect, it } from 'vitest'
import { layoutFlame } from './layout'
import type { FlameNode } from '../api'

const n = (name: string, total: number, children: FlameNode[] = []): FlameNode => ({
  funcId: 1, name, file: '', selfValue: 0, total, children,
})

describe('layoutFlame', () => {
  const a = n('a', 5, [n('a1', 5)])
  const c = n('c', 5)
  const root = n('root', 10, [a, c])

  it('lays out children proportionally', () => {
    const r = layoutFlame(root, 1000, 18, false)
    const ra = r.find((x) => x.node === a)!
    const rc = r.find((x) => x.node === c)!
    expect(ra.x).toBe(0); expect(ra.w).toBe(500)
    expect(rc.x).toBe(500); expect(rc.w).toBe(500)
  })
  it('omits rects narrower than 0.5px', () => {
    const tiny = n('tiny', 0.004)
    const r = layoutFlame(n('root', 1000, [tiny, n('big', 999.996)]), 1000, 18, false)
    expect(r.find((x) => x.node === tiny)).toBeUndefined()
  })
  it('zoom makes the node fill the width', () => {
    const r = layoutFlame(root, 1000, 18, false, a)
    expect(r.find((x) => x.node === a)!.w).toBe(1000)
  })
  it('icicle puts depth 1 at rowHeight; flame puts root at bottom', () => {
    const ic = layoutFlame(root, 1000, 18, true)
    expect(ic.find((x) => x.node === a)!.y).toBe(18)
    const fl = layoutFlame(root, 1000, 18, false)
    expect(fl.find((x) => x.node === root)!.y).toBe(36)
    expect(fl.find((x) => x.node === a1(a))!.y).toBe(0)
  })
})
const a1 = (a: FlameNode) => a.children[0]
