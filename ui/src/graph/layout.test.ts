import { describe, expect, it } from 'vitest'
import { layoutGraph, type LEdge, type LNode } from './layout'

const nodes = (n: number, w = 100, h = 30): LNode[] => Array.from({ length: n }, (_, i) => ({ id: i, w, h }))
const e = (from: number, to: number, weight = 1): LEdge => ({ from, to, weight })
const by = (r: ReturnType<typeof layoutGraph>, id: number) => r.nodes.find((n) => n.id === id)!

function noOverlapInLayers(r: ReturnType<typeof layoutGraph>, ns: LNode[], gap: number) {
  const w = new Map(ns.map((n) => [n.id, n.w]))
  const layers = new Map<number, { x: number; w: number }[]>()
  for (const p of r.nodes) {
    const l = layers.get(p.layer) ?? []
    l.push({ x: p.x, w: w.get(p.id)! })
    layers.set(p.layer, l)
  }
  for (const l of layers.values()) {
    l.sort((a, b) => a.x - b.x)
    for (let i = 1; i < l.length; i++) expect(l[i].x - (l[i - 1].x + l[i - 1].w)).toBeGreaterThanOrEqual(gap - 1e-9)
  }
}

describe('layoutGraph', () => {
  it('lays a chain out in increasing layers and y', () => {
    const r = layoutGraph(nodes(3), [e(0, 1), e(1, 2)])
    expect([0, 1, 2].map((i) => by(r, i).layer)).toEqual([0, 1, 2])
    expect(by(r, 0).y).toBeLessThan(by(r, 1).y)
    expect(by(r, 1).y).toBeLessThan(by(r, 2).y)
  })
  it('puts diamond siblings in one non-overlapping layer', () => {
    const ns = nodes(4)
    const r = layoutGraph(ns, [e(0, 1), e(0, 2), e(1, 3), e(2, 3)], { nodeGap: 20 })
    expect(by(r, 1).layer).toBe(by(r, 2).layer)
    noOverlapInLayers(r, ns, 20)
  })
  it('terminates on a cycle with one reversed route', () => {
    const r = layoutGraph(nodes(2), [e(0, 1), e(1, 0)])
    expect(r.routes.filter((x) => x.reversed)).toHaveLength(1)
    expect(r.routes).toHaveLength(2)
  })
  it('ignores self edges and unknown ids', () => {
    const r = layoutGraph(nodes(2), [e(0, 0), e(0, 9), e(0, 1)])
    expect(r.routes).toHaveLength(1)
  })
  it('routes a 3-layer edge through dummy positions', () => {
    const r = layoutGraph(nodes(4), [e(0, 1), e(1, 2), e(2, 3), e(0, 3)])
    const long = r.routes.find((x) => x.from === 0 && x.to === 3)!
    expect(long.points.length).toBeGreaterThanOrEqual(4)
  })
  it('places disconnected components without overlap', () => {
    const ns = nodes(4)
    const r = layoutGraph(ns, [e(0, 1), e(2, 3)], { nodeGap: 10 })
    noOverlapInLayers(r, ns, 10)
    expect(r.nodes).toHaveLength(4)
  })
  it('is deterministic', () => {
    const es = [e(0, 1), e(0, 2), e(1, 3), e(2, 3), e(3, 0)]
    expect(layoutGraph(nodes(4), es)).toEqual(layoutGraph(nodes(4), es))
  })
  it('empty input has zero size', () => {
    const r = layoutGraph([], [])
    expect([r.width, r.height, r.nodes.length, r.routes.length]).toEqual([0, 0, 0, 0])
  })
  it('lays out 300 nodes / 1200 seeded edges quickly without overlaps', () => {
    let seed = 12345
    const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32)
    const ns = nodes(300, 80, 24)
    const es: LEdge[] = []
    for (let i = 0; i < 1200; i++) es.push(e(Math.floor(rnd() * 300), Math.floor(rnd() * 300), 1 + Math.floor(rnd() * 9)))
    const t0 = performance.now()
    const r = layoutGraph(ns, es)
    const ms = performance.now() - t0
    console.log(`layout 300/1200: ${ms.toFixed(0)} ms`)
    expect(ms).toBeLessThan(300)
    noOverlapInLayers(r, ns, 20)
    expect(r.nodes).toHaveLength(300)
  })
  it('handles a 10 000-node chain', () => {
    const es: LEdge[] = []
    for (let i = 0; i < 9_999; i++) es.push(e(i, i + 1))
    const r = layoutGraph(nodes(10_000, 50, 10), es)
    expect(by(r, 9_999).layer).toBe(9_999)
  })
})
