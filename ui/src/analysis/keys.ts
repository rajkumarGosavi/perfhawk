import { RAIL } from '../shell/views'
import type { ViewName } from '../url'

export type KeyAction =
  | { type: 'view'; view: ViewName }
  | { type: 'hot' }
  | { type: 'search' }
  | { type: 'help' }
  | { type: 'history' }

/** Profile views by key; History is not a profile view and has its own action. */
const VIEW_KEYS: Record<string, ViewName> = Object.fromEntries(RAIL.filter((r) => r.view !== 'history').map((r) => [r.key, r.view]))

type KeyEventLike = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
  target: object | null
}

export function keyAction(e: KeyEventLike): KeyAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null
  const t = e.target as { tagName?: string; isContentEditable?: boolean } | null
  if (t && (t.isContentEditable || t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) {
    return null
  }
  if (Object.hasOwn(VIEW_KEYS, e.key)) return { type: 'view', view: VIEW_KEYS[e.key] }
  if (e.key === RAIL.find((r) => r.view === 'history')!.key) return { type: 'history' }
  if (e.key === 'h') return { type: 'hot' }
  if (e.key === '/') return { type: 'search' }
  if (e.key === '?') return { type: 'help' }
  return null
}

export const SHORTCUTS: { keys: string; label: string }[] = [
  ...RAIL.map((r) => ({ keys: r.key, label: r.needsTimeline ? `${r.label} (profiles with time data)` : r.label })),
  { keys: 'Ctrl K', label: 'Command palette' },
  { keys: 'h', label: 'Toggle hot path' },
  { keys: '/', label: 'Focus search' },
  { keys: '?', label: 'Show this help' },
  { keys: 'Esc', label: 'Reset zoom (flame) / close help' },
  { keys: 'f m x', label: 'Focus subtree / Merge / Drop the selected call-tree node' },
  { keys: '↑ ↓ ← → Enter', label: 'Navigate the call tree' },
]
