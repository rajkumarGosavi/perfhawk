import { useState } from 'react'
import { addTransform, chipLabel, decodeTx, encodeTx, removeTransform, type Transform } from '../analysis/transforms'

type Props = { tx: string | undefined; onChange: (tx: string | undefined) => void }
type NamedKind = 'sub' | 'merge' | 'drop'

export function TransformBar({ tx, onChange }: Props) {
  const list = decodeTx(tx)
  const [kind, setKind] = useState<NamedKind>('sub')
  const [name, setName] = useState('')
  const commit = (next: Transform[]) => onChange(encodeTx(next))
  const has = (k: 'rec' | 'pkg') => list.some((t) => t.kind === k)
  const toggle = (k: 'rec' | 'pkg') =>
    commit(has(k) ? list.filter((t) => t.kind !== k) : addTransform(list, { kind: k }))
  const add = () => {
    const n = name.trim()
    if (!n) return
    commit(addTransform(list, { kind, name: n }))
    setName('')
  }
  return (
    <div className="toolbar transforms" role="group" aria-label="Transforms">
      <ul className="chips" aria-label="Active transforms">
        {list.map((t, i) => (
          <li key={`${chipLabel(t)}-${i}`} className="chip">
            <span className="mono">{chipLabel(t)}</span>
            <button aria-label={`Remove ${chipLabel(t)}`} onClick={() => commit(removeTransform(list, i))}>
              ×
            </button>
          </li>
        ))}
      </ul>
      <button aria-pressed={has('rec')} onClick={() => toggle('rec')}>Collapse recursion</button>
      <button aria-pressed={has('pkg')} onClick={() => toggle('pkg')}>Collapse packages</button>
      <label className="field">
        Transform
        <select value={kind} onChange={(e) => setKind(e.target.value as NamedKind)}>
          <option value="sub">Focus subtree</option>
          <option value="merge">Merge</option>
          <option value="drop">Drop</option>
        </select>
      </label>
      <label className="field">
        Function name
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') add() }}
          placeholder="exact name"
        />
      </label>
      <button onClick={add} disabled={!name.trim()}>Add</button>
    </div>
  )
}
