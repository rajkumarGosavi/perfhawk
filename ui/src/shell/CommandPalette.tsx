import { Command as Cmdk } from 'cmdk'
import { useEffect, useRef } from 'react'
import type { Command } from './commands'

/** Ctrl K palette (cmdk on Radix Dialog): fuzzy filter, Enter runs, Esc closes, focus returns to the opener. */
export function CommandPalette({
  open, onOpenChange, commands,
}: { open: boolean; onOpenChange: (o: boolean) => void; commands: Command[] }) {
  // Radix only restores focus to a Dialog.Trigger; the palette has none, so remember the opener.
  const opener = useRef<HTMLElement | null>(null)
  useEffect(() => {
    if (open) {
      opener.current = (document.activeElement as HTMLElement | null) ?? null
      return
    }
    const o = opener.current
    opener.current = null
    if (o && o.isConnected) setTimeout(() => o.focus(), 0)
  }, [open])

  const groups = ['Views', 'App'] as const
  return (
    <Cmdk.Dialog
      open={open}
      onOpenChange={onOpenChange}
      label="Command palette"
      overlayClassName="palette-overlay"
      contentClassName="palette"
    >
      <Cmdk.Input placeholder="Type a command or view name…" className="palette-input" />
      <Cmdk.List className="palette-list">
        <Cmdk.Empty className="palette-empty">No matching commands</Cmdk.Empty>
        {groups.map((g) => {
          const items = commands.filter((c) => c.group === g)
          if (items.length === 0) return null
          return (
            <Cmdk.Group key={g} heading={g} className="palette-group">
              {items.map((c) => (
                <Cmdk.Item key={c.id} value={c.label} className="palette-item" onSelect={() => { onOpenChange(false); c.run() }}>
                  <span>{c.label}</span>
                  {c.hint && <kbd className="palette-hint">{c.hint}</kbd>}
                </Cmdk.Item>
              ))}
            </Cmdk.Group>
          )
        })}
      </Cmdk.List>
    </Cmdk.Dialog>
  )
}
