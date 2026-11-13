/** Small layered (Sugiyama-style) graph layout. Pure, deterministic and iterative. */

export type LNode = { id: number; w: number; h: number }
export type LEdge = { from: number; to: number; weight: number }
/** x, y are the top-left corner. */
export type Placed = { id: number; x: number; y: number; layer: number }
/** Control points from the original `from` node to the original `to` node. */
export type Route = { from: number; to: number; points: [number, number][]; reversed: boolean }

export type LayoutOptions = { layerGap?: number; nodeGap?: number; sweeps?: number }
export type LayoutResult = { nodes: Placed[]; routes: Route[]; width: number; height: number }

type Chain = { from: number; to: number; reversed: boolean; items: number[] }

export function layoutGraph(nodes: LNode[], edges: LEdge[], opts: LayoutOptions = {}): LayoutResult {
  const layerGap = opts.layerGap ?? 60
  const nodeGap = opts.nodeGap ?? 24
  const sweeps = opts.sweeps ?? 4
  const n = nodes.length
  if (n === 0) return { nodes: [], routes: [], width: 0, height: 0 }

  const index = new Map<number, number>()
  nodes.forEach((nd, i) => index.set(nd.id, i))

  // Usable edges as (index from, index to), in input order.
  const es: { a: number; b: number; from: number; to: number }[] = []
  for (const e of edges) {
    const a = index.get(e.from)
    const b = index.get(e.to)
    if (a === undefined || b === undefined || a === b) continue
    es.push({ a, b, from: e.from, to: e.to })
  }

  // Break cycles: iterative DFS in input order, reversing back edges.
  const adj: number[][] = Array.from({ length: n }, () => [])
  es.forEach((e, i) => adj[e.a].push(i))
  const state = new Uint8Array(n) // 0 new, 1 on stack, 2 done
  const reversed = new Uint8Array(es.length)
  for (let s = 0; s < n; s++) {
    if (state[s] !== 0) continue
    const stack: [number, number][] = [[s, 0]]
    state[s] = 1
    while (stack.length) {
      const top = stack[stack.length - 1]
      const [v, k] = top
      if (k < adj[v].length) {
        top[1]++
        const ei = adj[v][k]
        const w = es[ei].b
        if (state[w] === 1) reversed[ei] = 1
        else if (state[w] === 0) {
          state[w] = 1
          stack.push([w, 0])
        }
      } else {
        state[v] = 2
        stack.pop()
      }
    }
  }

  // Layering: longest path over the DAG (Kahn's algorithm).
  const dag = es.map((e, i) => (reversed[i] ? { u: e.b, v: e.a } : { u: e.a, v: e.b }))
  const outs: number[][] = Array.from({ length: n }, () => [])
  const indeg = new Int32Array(n)
  for (const d of dag) {
    outs[d.u].push(d.v)
    indeg[d.v]++
  }
  const layerOf: number[] = new Array(n).fill(0)
  const queue: number[] = []
  for (let i = 0; i < n; i++) if (indeg[i] === 0) queue.push(i)
  for (let qi = 0; qi < queue.length; qi++) {
    const u = queue[qi]
    for (const v of outs[u]) {
      if (layerOf[u] + 1 > layerOf[v]) layerOf[v] = layerOf[u] + 1
      if (--indeg[v] === 0) queue.push(v)
    }
  }

  // Items: real nodes 0..n-1, then dummies. Chains connect adjacent layers.
  const itemLayer: number[] = layerOf.slice()
  const itemW: number[] = nodes.map((nd) => nd.w)
  const upN: number[][] = Array.from({ length: n }, () => []) // neighbours in the layer above
  const downN: number[][] = Array.from({ length: n }, () => [])
  const chains: Chain[] = []
  es.forEach((e, i) => {
    const u = dag[i].u
    const v = dag[i].v
    const items = [u]
    for (let l = layerOf[u] + 1; l < layerOf[v]; l++) {
      const d = itemLayer.length
      itemLayer.push(l)
      itemW.push(1)
      upN.push([])
      downN.push([])
      items.push(d)
    }
    items.push(v)
    for (let k = 0; k + 1 < items.length; k++) {
      downN[items[k]].push(items[k + 1])
      upN[items[k + 1]].push(items[k])
    }
    chains.push({ from: e.from, to: e.to, reversed: !!reversed[i], items })
  })

  const layerCount = itemLayer.reduce((m, l) => (l > m ? l : m), 0) + 1
  const layers: number[][] = Array.from({ length: layerCount }, () => [])
  for (let i = 0; i < itemLayer.length; i++) layers[itemLayer[i]].push(i)
  const pos = new Float64Array(itemLayer.length)
  const refresh = (l: number) => layers[l].forEach((it, p) => (pos[it] = p))
  for (let l = 0; l < layerCount; l++) refresh(l)

  const reorder = (l: number, neigh: number[][]) => {
    const keyed = layers[l].map((it) => {
      const ns = neigh[it]
      const bc = ns.length ? ns.reduce((s, x) => s + pos[x], 0) / ns.length : pos[it]
      return { it, bc, p: pos[it] }
    })
    keyed.sort((x, y) => x.bc - y.bc || x.p - y.p)
    layers[l] = keyed.map((k) => k.it)
    refresh(l)
  }
  for (let s = 0; s < sweeps; s++) {
    for (let l = 1; l < layerCount; l++) reorder(l, upN)
    for (let l = layerCount - 2; l >= 0; l--) reorder(l, downN)
  }

  // Horizontal packing, each layer centred on the widest one.
  const x: number[] = new Array(itemLayer.length).fill(0)
  const widths = layers.map((layer) => layer.reduce((s, it) => s + itemW[it], 0) + Math.max(0, layer.length - 1) * nodeGap)
  const maxWidth = widths.reduce((m, w) => (w > m ? w : m), 0)
  layers.forEach((layer, l) => {
    let cx = (maxWidth - widths[l]) / 2
    for (const it of layer) {
      x[it] = cx
      cx += itemW[it] + nodeGap
    }
  })

  const maxH = nodes.reduce((m, nd) => (nd.h > m ? nd.h : m), 0)
  const pitch = maxH + layerGap
  const centreX = (it: number) => x[it] + itemW[it] / 2
  const placed: Placed[] = nodes.map((nd, i) => ({ id: nd.id, x: x[i], y: layerOf[i] * pitch, layer: layerOf[i] }))

  const routes: Route[] = chains.map((c) => {
    const pts: [number, number][] = c.items.map((it, k) => {
      if (k === 0) return [centreX(it), itemLayer[it] * pitch + nodes[it].h]
      if (k === c.items.length - 1) return [centreX(it), itemLayer[it] * pitch]
      return [centreX(it), itemLayer[it] * pitch + maxH / 2]
    })
    return { from: c.from, to: c.to, points: c.reversed ? pts.reverse() : pts, reversed: c.reversed }
  })

  return { nodes: placed, routes, width: maxWidth, height: (layerCount - 1) * pitch + maxH }
}
