import * as Menu from '@radix-ui/react-dropdown-menu'

export type ExportItem = { label: string; run: () => void | Promise<void> }

/** A menu button listing export actions (Radix DropdownMenu: arrows/Home/End, Esc, focus return, outside click). */
export function ExportMenu({ items }: { items: ExportItem[] }) {
  return (
    <Menu.Root>
      <Menu.Trigger asChild>
        <button>Export</button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className="menu" aria-label="Export" align="start" sideOffset={4}>
          {items.map((it) => (
            <Menu.Item key={it.label} className="menu-item" onSelect={() => { void it.run() }}>
              {it.label}
            </Menu.Item>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  )
}
