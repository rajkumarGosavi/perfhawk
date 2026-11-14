export const WELCOME_KEY = 'perfhawk.welcome.dismissed'

/**
 * Show the first-run panel until the user dismisses it, and again whenever there is nothing
 * to look at. `itemCount` is sessions plus profiles, or null while still loading.
 * Any stored value counts as dismissed.
 */
export function shouldShowWelcome(dismissed: string | null, itemCount: number | null): boolean {
  if (dismissed === null) return true
  return itemCount === 0
}

export function recordCommand(pid: string): string {
  return `perfhawk record --pid ${pid.trim() || '<pid>'} --open`
}
