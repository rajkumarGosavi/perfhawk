import { useCallback, useEffect, useRef, useState } from 'react'
import { mergeViewState, readViewState, writeViewState, type ViewState } from './url'
import { DiffView } from './views/DiffView'
import { Home } from './views/Home'
import { ProfileView } from './views/ProfileView'

export default function App() {
  const [state, setState] = useState<ViewState>(() => readViewState(window.location.search))

  const ref = useRef(state)

  useEffect(() => {
    const onPop = () => {
      ref.current = readViewState(window.location.search)
      setState(ref.current)
    }
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  // push = new history entry (view/profile changes); replace = filter typing.
  // The patch is merged into the *latest* state, so deferred callers (debounce
  // timers) cannot clobber newer edits. `reset` replaces the state wholesale.
  const navigate = useCallback((patch: Partial<ViewState>, mode: 'push' | 'replace' = 'push', reset = false) => {
    const next = reset ? ({ view: 'summary', ...patch } as ViewState) : mergeViewState(ref.current, patch)
    ref.current = next
    const url = window.location.pathname + writeViewState(next)
    if (mode === 'push') window.history.pushState(null, '', url)
    else window.history.replaceState(null, '', url)
    setState(next)
  }, [])

  return (
    <>
      <header className="app-header">
        <h1>
          <button className="linklike" onClick={() => navigate({}, 'push', true)}>
            PRISM
          </button>
        </h1>
        <span className="muted">local profiling workbench</span>
      </header>
      <main>
        {state.profile && state.view === 'diff' ? (
          <DiffView key={`${state.profile}|${state.base ?? ''}`} state={state} navigate={navigate} />
        ) : state.profile ? (
          <ProfileView key={state.profile} state={state} navigate={navigate} />
        ) : (
          <Home
            session={state.session}
            onOpen={(id) => navigate({ profile: id }, 'push', true)}
            onOpenWindow={(id, r) => navigate({ profile: id, view: 'flame', t0: r.t0, t1: r.t1 }, 'push', true)}
          />
        )}
      </main>
    </>
  )
}
