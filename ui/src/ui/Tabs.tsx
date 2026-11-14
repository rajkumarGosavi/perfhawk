import * as RadixTabs from '@radix-ui/react-tabs'
import type { ReactNode } from 'react'

export type TabDef = { value: string; label: ReactNode; panel: ReactNode }

/** Tabs on Radix: roving focus, Left/Right/Home/End, automatic activation. Only the active panel is mounted. */
export function Tabs({
  label, value, onValueChange, tabs, between,
}: {
  label: string
  value: string
  onValueChange: (v: string) => void
  tabs: TabDef[]
  /** Rendered between the tab list and the panels (e.g. a status line). */
  between?: ReactNode
}) {
  return (
    <RadixTabs.Root value={value} onValueChange={onValueChange}>
      <RadixTabs.List className="tabs-list" aria-label={label}>
        {tabs.map((t) => (
          <RadixTabs.Trigger key={t.value} value={t.value} className="tabs-trigger">{t.label}</RadixTabs.Trigger>
        ))}
      </RadixTabs.List>
      {between}
      {tabs.map((t) => (
        <RadixTabs.Content key={t.value} value={t.value} className="tabs-panel">{t.panel}</RadixTabs.Content>
      ))}
    </RadixTabs.Root>
  )
}
