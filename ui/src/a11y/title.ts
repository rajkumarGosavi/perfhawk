import type { ViewName, ViewState } from '../url'

const VIEW_TITLES: Record<ViewName, string> = {
  summary: 'Summary',
  flame: 'Flame graph',
  icicle: 'Icicle graph',
  tree: 'Call tree',
  top: 'Top functions',
  both: 'Flame graph and top functions',
  sandwich: 'Sandwich view',
  diff: 'Diff',
  groups: 'Groups',
  graph: 'Call graph',
  time: 'Time heat map',
  history: 'History',
}

const APP = 'Perfhawk'

/** The page title for the current screen, so screen-reader and tab users can tell where they are. */
export function viewTitle(state: Pick<ViewState, 'view' | 'profile' | 'base'>, profileName?: string): string {
  if (!state.profile) return state.view === 'history' ? `${VIEW_TITLES.history} · ${APP}` : APP
  return `${VIEW_TITLES[state.view]} · ${profileName ?? state.profile} · ${APP}`
}
