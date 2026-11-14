import type { Nav } from '../nav'
import type { ViewState } from '../url'
import { RAIL, railView, type RailItem } from './views'

const GROUPS: { id: RailItem['group']; title: string }[] = [
  { id: 'analyze', title: 'Analyze' },
  { id: 'compare', title: 'Compare' },
]

/** Left rail: the views of the open profile, grouped, with their shortcut keys. */
export function Rail({
  state, navigate, hasTime, hasGc = false, diffBase,
}: { state: ViewState; navigate: Nav; hasTime: boolean; hasGc?: boolean; diffBase?: string }) {
  const active = railView(state.view)
  return (
    <nav className="rail" aria-label="Views">
      {GROUPS.map((g) => (
        <div key={g.id} className="rail-group" role="group" aria-label={g.title}>
          <div className="rail-title" aria-hidden="true">{g.title}</div>
          {RAIL.filter((r) => r.group === g.id).map((r) => (
            <button
              key={r.view}
              type="button"
              className="rail-item"
              aria-current={active === r.view ? 'page' : undefined}
              disabled={(!!r.needsTimeline && !hasTime) || (!!r.needsGc && !hasGc) || (!!r.needsDiff && !diffBase)}
              onClick={() => (
                r.view === 'history' ? navigate({ view: 'history' }, 'push', true)
                : r.view === 'diff' ? navigate({ view: 'diff', base: state.base ?? diffBase, t0: undefined, t1: undefined, b0: undefined, b1: undefined }, 'push')
                : navigate({ view: r.view }, 'push')
              )}
            >
              <span>{r.label}</span>
              <kbd className="rail-key" aria-hidden="true">{r.key}</kbd>
            </button>
          ))}
        </div>
      ))}
    </nav>
  )
}
