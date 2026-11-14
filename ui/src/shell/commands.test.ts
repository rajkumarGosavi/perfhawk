import { describe, expect, it, vi } from 'vitest'
import { buildCommands, type CommandCtx } from './commands'

const ctx = (over: Partial<CommandCtx> = {}): CommandCtx => ({
  hasProfile: true, hasTimeline: true,
  navigate: vi.fn(), openHelp: vi.fn(), setTheme: vi.fn(), focusSearch: vi.fn(),
  ...over,
})
const labels = (c: CommandCtx) => buildCommands(c).map((x) => x.label)

describe('buildCommands', () => {
  it('without a profile offers History, Home, Help and themes but no profile views', () => {
    const l = labels(ctx({ hasProfile: false }))
    expect(l).toContain('Go to History')
    expect(l).toContain('Go to home')
    expect(l).toContain('Open help')
    expect(l).toContain('Theme: Dark')
    expect(l).not.toContain('Go to Flame graph')
    expect(l).not.toContain('Focus search')
  })
  it('with a profile but no timeline omits Timeline', () => {
    const l = labels(ctx({ hasTimeline: false }))
    expect(l).toContain('Go to Flame graph')
    expect(l).toContain('Focus search')
    expect(l).not.toContain('Go to Timeline')
  })
  it('runs navigation, theme, help and search actions', () => {
    const c = ctx()
    const by = (label: string) => buildCommands(c).find((x) => x.label === label)!
    by('Go to Flame graph').run()
    expect(c.navigate).toHaveBeenCalledWith({ view: 'flame' }, 'push')
    by('Go to History').run()
    expect(c.navigate).toHaveBeenCalledWith({ view: 'history' }, 'push', true)
    by('Go to home').run()
    expect(c.navigate).toHaveBeenCalledWith({}, 'push', true)
    by('Theme: Light').run()
    expect(c.setTheme).toHaveBeenCalledWith('light')
    by('Open help').run()
    expect(c.openHelp).toHaveBeenCalled()
    by('Focus search').run()
    expect(c.focusSearch).toHaveBeenCalled()
  })
  it('has unique ids', () => {
    const ids = buildCommands(ctx()).map((x) => x.id)
    expect(new Set(ids).size).toBe(ids.length)
  })
})
