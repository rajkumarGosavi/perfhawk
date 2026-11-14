import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'

/** What the open profile tells the shell: the breadcrumb label and whether Timeline is available. */
export type ProfileInfo = {
  name: string
  hasTime: boolean
  /** The profile's session has GC data. */
  hasGc?: boolean
  /** Short id of a profile to compare with (newest of the same kind), when one exists. */
  diffBase?: string
}

type Ctx = { info: ProfileInfo | null; setInfo: (i: ProfileInfo | null) => void }
const ShellCtx = createContext<Ctx>({ info: null, setInfo: () => {} })

export function ShellProvider({ children }: { children: ReactNode }) {
  const [info, setInfo] = useState<ProfileInfo | null>(null)
  const value = useMemo(() => ({ info, setInfo }), [info])
  return <ShellCtx.Provider value={value}>{children}</ShellCtx.Provider>
}

/** Registers the profile for the lifetime of the calling component; pass null while it is loading. */
export function useProfileInfo(info: ProfileInfo | null): void {
  const { setInfo } = useContext(ShellCtx)
  const name = info?.name
  const hasTime = info?.hasTime
  const hasGc = info?.hasGc
  const diffBase = info?.diffBase
  useEffect(() => {
    setInfo(name === undefined ? null : { name, hasTime: !!hasTime, hasGc: !!hasGc, diffBase })
    return () => setInfo(null)
  }, [setInfo, name, hasTime, hasGc, diffBase])
}

export function useShellInfo(): ProfileInfo | null {
  return useContext(ShellCtx).info
}
