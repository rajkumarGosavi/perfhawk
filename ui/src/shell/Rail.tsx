import type { Nav } from '../nav'
import type { ViewState } from '../url'
import { RAIL, railView, type RailItem } from './views'

const GROUPS: { id: RailItem['group']; title: string }[] = [
  { id: 'analyze', title: 'Analyze' },
  { id: 'compare', title: 'Compare' },
]

/** Left rail: the views of the open profile, grouped, with their shortcut keys. */
export function Rail({ state, navigate, hasTime }: { state: ViewState; navigate: Nav; hasTime: boolean }) {
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
              disabled={!!r.needsTimeline && !hasTime}
              onClick={() => (r.view === 'history' ? navigate({ view: 'history' }, 'push', true) : navigate({ view: r.view }, 'push'))}
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
