import { useCallback, useMemo, useState, type ReactNode } from 'react'
import { AnnounceContext, type Announce } from './announce'

/** One polite live region for the whole app; any component can `useAnnounce()` a short message. */
export function AnnouncerProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState('')
  const announce = useCallback<Announce>((m) => {
    // Clear first so repeating the same message is announced again.
    setMessage('')
    setTimeout(() => setMessage(m), 50)
  }, [])
  const value = useMemo(() => announce, [announce])
  return (
    <AnnounceContext.Provider value={value}>
      {children}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{message}</div>
    </AnnounceContext.Provider>
  )
}
