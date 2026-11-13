/**
 * Colour for a frame by how its value changed between base and head.
 * Red = grew, green = shrank, grey = unchanged. The values are CSS custom
 * properties (see theme.css); the canvas renderer resolves them.
 */
export function diffColor(base: number, head: number): string {
  const ratio = Math.max(-1, Math.min(1, (head - base) / Math.max(base, head, 1)))
  if (ratio === 0) return 'var(--diff-flat)'
  const bucket = Math.min(4, Math.floor(Math.abs(ratio) * 5))
  return ratio > 0 ? `var(--diff-up-${bucket})` : `var(--diff-down-${bucket})`
}
