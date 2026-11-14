import { createContext, useContext, useEffect } from 'react'

export type Announce = (message: string) => void

export const AnnounceContext = createContext<Announce>(() => {})

export const useAnnounce = () => useContext(AnnounceContext)

/** Sets the document title and announces it (a page change, for screen readers). */
export function useDocumentTitle(title: string) {
  const announce = useAnnounce()
  useEffect(() => {
    document.title = title
    announce(title)
  }, [title, announce])
}
