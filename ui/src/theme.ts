import { getPref, setPref } from './storage'

export type ThemePref = 'system' | 'light' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

const KEY = 'perfhawk.theme'
const EVENT = 'perfhawk-theme'

export function parseThemePref(raw: string | null): ThemePref {
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system'
}

export function resolveTheme(pref: ThemePref, systemDark: boolean): ResolvedTheme {
  return pref === 'system' ? (systemDark ? 'dark' : 'light') : pref
}

export function readThemePref(): ThemePref {
  return parseThemePref(getPref(KEY))
}

const systemDark = () => typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches

/** The theme currently in effect (what `data-theme` says), for canvas drawing and such. */
export function currentTheme(): ResolvedTheme {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light'
}

/** Sets `data-theme` on <html> to the resolved theme; CSS only ever looks at that attribute. */
export function applyThemePref(pref: ThemePref): void {
  document.documentElement.dataset.theme = resolveTheme(pref, systemDark())
  window.dispatchEvent(new Event(EVENT))
}

export function writeThemePref(pref: ThemePref): void {
  setPref(KEY, pref)
  applyThemePref(pref)
}

/** Apply the saved preference now and follow OS changes while the preference is "system". */
export function initTheme(): void {
  applyThemePref(readThemePref())
  if (typeof matchMedia === 'function') {
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (readThemePref() === 'system') applyThemePref('system')
    })
  }
}

export function subscribeTheme(cb: () => void): () => void {
  window.addEventListener(EVENT, cb)
  return () => window.removeEventListener(EVENT, cb)
}
