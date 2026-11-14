import type { ViewState } from './url'

/** Navigate to a view state: `push` adds a history entry, `replace` rewrites it; `reset` drops the old state. */
export type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void
