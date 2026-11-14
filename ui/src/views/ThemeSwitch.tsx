import { useState } from 'react'
import { readThemePref, writeThemePref, type ThemePref } from '../theme'

const OPTIONS: { value: ThemePref; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

/** Three-way colour theme control; "System" follows the operating system setting. */
export function ThemeSwitch() {
  const [pref, setPref] = useState<ThemePref>(readThemePref)
  return (
    <div className="tabs theme-switch" role="group" aria-label="Colour theme">
      {OPTIONS.map((o) => (
        <button
          key={o.value}
          aria-pressed={pref === o.value}
          onClick={() => { setPref(o.value); writeThemePref(o.value) }}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}
