/** Stack transforms carried in the `tx` URL/query parameter (see docs/VISUALIZATION.md A5). */
export type Transform =
  | { kind: 'sub' | 'merge' | 'drop'; name: string }
  | { kind: 'rec' }
  | { kind: 'pkg' }

const MAX = 32

function decodeItem(item: string): Transform | undefined {
  const i = item.indexOf(':')
  const kind = i < 0 ? item : item.slice(0, i)
  const payload = i < 0 ? undefined : item.slice(i + 1)
  if (kind === 'rec' || kind === 'pkg') return payload === undefined ? { kind } : undefined
  if (kind === 'sub' || kind === 'merge' || kind === 'drop') {
    if (!payload) return undefined
    try {
      const name = decodeURIComponent(payload)
      return name ? { kind, name } : undefined
    } catch {
      return undefined
    }
  }
  return undefined
}

export function decodeTx(s: string | undefined): Transform[] {
  if (!s) return []
  const out: Transform[] = []
  for (const item of s.split(',')) {
    const t = decodeItem(item)
    if (t) out.push(t)
    if (out.length === MAX) break
  }
  return out
}

export function encodeTx(list: Transform[]): string | undefined {
  if (list.length === 0) return undefined
  return list
    .slice(0, MAX)
    .map((t) => ('name' in t ? `${t.kind}:${encodeURIComponent(t.name)}` : t.kind))
    .join(',')
}

export function chipLabel(t: Transform): string {
  switch (t.kind) {
    case 'sub': return `Focus subtree: ${t.name}`
    case 'merge': return `Merge: ${t.name}`
    case 'drop': return `Drop: ${t.name}`
    case 'rec': return 'Collapse recursion'
    case 'pkg': return 'Collapse packages'
  }
}

const same = (a: Transform, b: Transform) =>
  a.kind === b.kind && ('name' in a ? 'name' in b && a.name === b.name : true)

export function addTransform(list: Transform[], t: Transform): Transform[] {
  if (list.some((x) => same(x, t)) || list.length >= MAX) return [...list]
  return [...list, t]
}

export function removeTransform(list: Transform[], index: number): Transform[] {
  return list.filter((_, i) => i !== index)
}
