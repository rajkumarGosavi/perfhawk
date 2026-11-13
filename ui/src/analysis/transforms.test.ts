import { describe, expect, it } from 'vitest'
import { addTransform, chipLabel, decodeTx, encodeTx, removeTransform, type Transform } from './transforms'

describe('transforms', () => {
  it('roundtrips names containing , : % space and /', () => {
    const list: Transform[] = [
      { kind: 'sub', name: 'a,b:c%d /x' },
      { kind: 'rec' },
      { kind: 'pkg' },
      { kind: 'merge', name: 'm' },
    ]
    const s = encodeTx(list)!
    expect(s.split(',')).toHaveLength(4)
    expect(decodeTx(s)).toEqual(list)
  })
  it('empty list encodes to undefined, undefined decodes to empty', () => {
    expect(encodeTx([])).toBeUndefined()
    expect(decodeTx(undefined)).toEqual([])
    expect(decodeTx('')).toEqual([])
  })
  it('drops malformed items', () => {
    expect(decodeTx('bogus,sub:,rec:x,merge:%zz,drop:ok')).toEqual([{ kind: 'drop', name: 'ok' }])
  })
  it('caps at 32 items', () => {
    expect(decodeTx(Array(33).fill('rec').join(','))).toHaveLength(32)
  })
  it('addTransform dedupes and keeps rec/pkg single', () => {
    const a: Transform[] = [{ kind: 'sub', name: 'x' }, { kind: 'rec' }]
    expect(addTransform(a, { kind: 'sub', name: 'x' })).toEqual(a)
    expect(addTransform(a, { kind: 'rec' })).toEqual(a)
    const b = addTransform(a, { kind: 'drop', name: 'x' })
    expect(b).toHaveLength(3)
    expect(b).not.toBe(a)
  })
  it('removeTransform removes by index without mutating', () => {
    const a: Transform[] = [{ kind: 'rec' }, { kind: 'pkg' }]
    expect(removeTransform(a, 0)).toEqual([{ kind: 'pkg' }])
    expect(a).toHaveLength(2)
  })
  it('chipLabel', () => {
    expect(chipLabel({ kind: 'sub', name: 'X' })).toBe('Focus subtree: X')
    expect(chipLabel({ kind: 'merge', name: 'X' })).toBe('Merge: X')
    expect(chipLabel({ kind: 'drop', name: 'X' })).toBe('Drop: X')
    expect(chipLabel({ kind: 'rec' })).toBe('Collapse recursion')
    expect(chipLabel({ kind: 'pkg' })).toBe('Collapse packages')
  })
})
