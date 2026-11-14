import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'

/** What the open profile tells the shell: the breadcrumb label and whether Timeline is available. */
export type ProfileInfo = { name: string; hasTime: boolean }

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
  useEffect(() => {
    setInfo(name === undefined ? null : { name, hasTime: !!hasTime })
    return () => setInfo(null)
  }, [setInfo, name, hasTime])
}

export function useShellInfo(): ProfileInfo | null {
  return useContext(ShellCtx).info
}
