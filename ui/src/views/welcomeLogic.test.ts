import { describe, expect, it } from 'vitest'
import { recordCommand, shouldShowWelcome } from './welcomeLogic'

describe('shouldShowWelcome', () => {
  it('shows when never dismissed', () => {
    expect(shouldShowWelcome(null, 3)).toBe(true)
    expect(shouldShowWelcome(null, null)).toBe(true)
  })
  it('hides once dismissed and there is data', () => {
    expect(shouldShowWelcome('1', 3)).toBe(false)
  })
  it('shows again when dismissed but there is nothing to look at', () => {
    expect(shouldShowWelcome('1', 0)).toBe(true)
  })
  it('treats any stored value as dismissed (garbage in storage)', () => {
    expect(shouldShowWelcome('maybe', 3)).toBe(false)
  })
  it('does not flash the welcome while the counts are still loading for a dismissed user', () => {
    expect(shouldShowWelcome('1', null)).toBe(false)
  })
})

describe('recordCommand', () => {
  it('builds the copyable command', () => {
    expect(recordCommand('1234')).toBe('perfhawk record --pid 1234 --open')
  })
  it('falls back to a placeholder', () => {
    expect(recordCommand('')).toBe('perfhawk record --pid <pid> --open')
  })
})
