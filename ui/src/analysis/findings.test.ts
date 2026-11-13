import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Finding, FindingAction } from '../api'
import { actionToPatch, loadDisabled, saveDisabled, visibleFindings } from './findings'

const act = (over: Partial<FindingAction>): FindingAction => ({
  label: 'x', view: 'flame', funcId: null, focus: null, tx: null, sort: null, groupBy: null, ...over,
})
const fnd = (rule: string): Finding => ({
  id: rule, rule, severity: 'warn', score: 1, title: rule, detail: '',
  evidence: { funcId: null, name: null, value: 0, pct: 0 }, action: act({}),
})

afterEach(() => vi.unstubAllGlobals())

function stubStorage(store: Record<string, string>, throws = false) {
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => { if (throws) throw new Error('blocked'); return store[k] ?? null },
    setItem: (k: string, v: string) => { if (throws) throw new Error('blocked'); store[k] = v },
  })
}

describe('actionToPatch', () => {
  it('maps a sandwich action', () => {
    expect(actionToPatch(act({ view: 'sandwich', funcId: 7 }))).toEqual({ view: 'sandwich', func: 7 })
  })
  it('maps a groups action with groupBy', () => {
    expect(actionToPatch(act({ view: 'groups', groupBy: 'site' }))).toEqual({ view: 'groups', groupBy: 'site' })
  })
  it('maps focus and tx', () => {
    expect(actionToPatch(act({ focus: '^a$', tx: 'rec' }))).toEqual({ view: 'flame', focus: '^a$', tx: 'rec' })
  })
  it('maps sort and ignores unknown groupBy', () => {
    expect(actionToPatch(act({ view: 'top', sort: 'total' }))).toEqual({ view: 'top', sort: 'total' })
    expect(actionToPatch(act({ groupBy: 'weird' }))).toEqual({ view: 'flame' })
  })
})

describe('disabled rules', () => {
  it('missing, corrupt and valid values', () => {
    stubStorage({})
    expect(loadDisabled().size).toBe(0)
    stubStorage({ 'perfhawk.findings.disabled': '{' })
    expect(loadDisabled().size).toBe(0)
    stubStorage({ 'perfhawk.findings.disabled': '["a","b"]' })
    expect([...loadDisabled()].sort()).toEqual(['a', 'b'])
    stubStorage({ 'perfhawk.findings.disabled': '{"a":1}' })
    expect(loadDisabled().size).toBe(0)
  })
  it('never throws when storage is blocked', () => {
    stubStorage({}, true)
    expect(loadDisabled().size).toBe(0)
    expect(() => saveDisabled(new Set(['a']))).not.toThrow()
  })
  it('roundtrips', () => {
    const store: Record<string, string> = {}
    stubStorage(store)
    saveDisabled(new Set(['x']))
    expect([...loadDisabled()]).toEqual(['x'])
  })
  it('visibleFindings hides disabled ids', () => {
    const all = [fnd('a'), fnd('b')]
    expect(visibleFindings(all, new Set(['a'])).map((f) => f.rule)).toEqual(['b'])
  })
})
