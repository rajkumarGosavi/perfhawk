export type Point = { x: number; y: number }

export function pointerDistance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y)
}

/** Zoom factor for a two-finger pinch, clamped to 0.25 .. 4; a pinch that began at zero distance is ignored. */
export function pinchScale(startDist: number, nowDist: number): number {
  if (!Number.isFinite(startDist) || !Number.isFinite(nowDist) || startDist <= 0) return 1
  return Math.min(4, Math.max(0.25, nowDist / startDist))
}
