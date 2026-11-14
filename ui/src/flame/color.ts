import { contrastRatio } from '../contrast'

export function hashHue(name: string): number {
  let h = 2166136261
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  // Warm range: red to yellow (0..55 degrees).
  return (h >>> 0) % 56
}

/** Black or white, whichever reads better on `fill` (`#hex` or `hsl()`). Text only; fills never change. */
export function labelColorFor(fill: string): '#000000' | '#ffffff' {
  try {
    return contrastRatio('#000000', fill) >= contrastRatio('#ffffff', fill) ? '#000000' : '#ffffff'
  } catch {
    return '#000000' // unparseable colour format: fall back to dark text
  }
}
