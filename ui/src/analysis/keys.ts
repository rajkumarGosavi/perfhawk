import type { ViewName } from '../url'

export type KeyAction =
  | { type: 'view'; view: ViewName }
  | { type: 'hot' }
  | { type: 'search' }
  | { type: 'help' }

const VIEW_KEYS: Record<string, ViewName> = {
  '1': 'summary',
  '2': 'flame',
  '3': 'icicle',
  '4': 'tree',
  '5': 'top',
  '6': 'both',
  '7': 'groups',
}

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
  if (e.key === 'h') return { type: 'hot' }
  if (e.key === '/') return { type: 'search' }
  if (e.key === '?') return { type: 'help' }
  return null
}

export const SHORTCUTS: { keys: string; label: string }[] = [
  { keys: '1', label: 'Summary' },
  { keys: '2', label: 'Flame graph' },
  { keys: '3', label: 'Icicle graph' },
  { keys: '4', label: 'Call tree' },
  { keys: '5', label: 'Top functions' },
  { keys: '6', label: 'Flame and top together' },
  { keys: '7', label: 'Groups (identical stacks / wait sites)' },
  { keys: 'h', label: 'Toggle hot path' },
  { keys: '/', label: 'Focus search' },
  { keys: '?', label: 'Show this help' },
  { keys: 'Esc', label: 'Reset zoom (flame) / close help' },
  { keys: 'f m x', label: 'Focus subtree / Merge / Drop the selected call-tree node' },
  { keys: '↑ ↓ ← → Enter', label: 'Navigate the call tree' },
]
