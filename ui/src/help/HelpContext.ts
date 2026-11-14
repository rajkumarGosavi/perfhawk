import { createContext, useContext } from 'react'

export type HelpApi = { open: (topic?: string) => void }

/** Lets any component open the Help panel; the App provides the real implementation. */
export const HelpContext = createContext<HelpApi>({ open: () => {} })

export const useHelp = () => useContext(HelpContext)
