import { describe, expect, it } from 'vitest'
import type { ViewName } from '../url'
import { GLOSSARY, HELP_TOPICS, topicForView, type TermKey } from './content'

const VIEWS: ViewName[] = ['summary', 'flame', 'icicle', 'tree', 'top', 'both', 'sandwich', 'diff', 'groups', 'graph', 'time', 'history']

describe('glossary', () => {
  it('has a label and a short definition for every term', () => {
    for (const [k, v] of Object.entries(GLOSSARY)) {
      expect(v.label.length, k).toBeGreaterThan(0)
      expect(v.short.length, k).toBeGreaterThan(0)
    }
  })
  it('defines self in terms of callees', () => {
    expect(GLOSSARY.self.short.toLowerCase()).toMatch(/calls|callees/)
  })
  it('covers the planned term keys', () => {
    const keys: TermKey[] = ['self', 'total', 'sampleType', 'fidelity', 'session', 'profile', 'window', 'baseline', 'hotPath', 'focus', 'exclude', 'hide', 'flame', 'icicle', 'sandwich']
    for (const k of keys) expect(GLOSSARY[k]).toBeDefined()
  })
})

describe('help topics', () => {
  it('has unique ids including the planned ones', () => {
    const ids = HELP_TOPICS.map((t) => t.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const id of ['getting-started', 'flame-graph', 'self-total', 'compare', 'shortcuts', 'troubleshooting']) {
      expect(ids).toContain(id)
    }
  })
  it('maps every view to an existing topic', () => {
    const ids = HELP_TOPICS.map((t) => t.id)
    for (const v of VIEWS) expect(ids).toContain(topicForView(v))
  })
})
