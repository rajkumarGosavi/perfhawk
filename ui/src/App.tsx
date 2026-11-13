import { useCallback, useEffect, useState } from 'react'
import { readViewState, writeViewState, type ViewState } from './url'
import { ProfileList } from './views/ProfileList'
import { ProfileView } from './views/ProfileView'

export default function App() {
  const [state, setState] = useState<ViewState>(() => readViewState(window.location.search))

  useEffect(() => {
    const onPop = () => setState(readViewState(window.location.search))
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  // push = new history entry (view/profile changes); replace = filter typing.
  const navigate = useCallback((next: ViewState, mode: 'push' | 'replace' = 'push') => {
    const url = window.location.pathname + writeViewState(next)
    if (mode === 'push') window.history.pushState(null, '', url)
    else window.history.replaceState(null, '', url)
    setState(next)
  }, [])

  return (
    <>
      <header className="app-header">
        <h1>
          <button className="linklike" onClick={() => navigate({ view: 'flame' })}>
            PRISM
          </button>
        </h1>
        <span className="muted">local profiling workbench</span>
      </header>
      <main>
        {state.profile ? (
          <ProfileView key={state.profile} state={state} navigate={navigate} />
        ) : (
          <ProfileList onOpen={(id) => navigate({ view: 'flame', profile: id })} />
        )}
      </main>
    </>
  )
}
