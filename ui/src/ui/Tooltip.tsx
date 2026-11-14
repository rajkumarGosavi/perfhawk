import * as RadixTooltip from '@radix-ui/react-tooltip'
import type { ReactElement, ReactNode } from 'react'

/** Wrap the app once so tooltips share open/close delays. */
export function TooltipProvider({ children }: { children: ReactNode }) {
  return <RadixTooltip.Provider delayDuration={300}>{children}</RadixTooltip.Provider>
}

/** A short label for an icon-only control; opens on hover and keyboard focus, Esc dismisses. */
export function Tooltip({ content, children }: { content: string; children: ReactElement }) {
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content className="tooltip-pop" sideOffset={6}>{content}</RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  )
}
