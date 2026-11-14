import { describe, expect, it } from 'vitest'
import { GLOSSARY, HELP_TOPICS, topicForTerm } from './content'

const sources = import.meta.glob('../**/*.tsx', { query: '?raw', import: 'default', eager: true }) as Record<string, string>

const used = new Set<string>()
for (const text of Object.values(sources)) {
  for (const m of text.matchAll(/<Term\s+k="([A-Za-z]+)"/g)) used.add(m[1])
}

describe('<Term> usage', () => {
  it('uses the glossary in the app (self, total and more)', () => {
    expect(used.has('self')).toBe(true)
    expect(used.has('total')).toBe(true)
    expect(used.size).toBeGreaterThanOrEqual(5)
  })
  it('only references keys that exist in the glossary', () => {
    for (const k of used) expect(Object.keys(GLOSSARY), k).toContain(k)
  })
  it('links every term to an existing help topic', () => {
    const ids = HELP_TOPICS.map((t) => t.id)
    for (const k of Object.keys(GLOSSARY)) expect(ids).toContain(topicForTerm(k as keyof typeof GLOSSARY))
  })
})
