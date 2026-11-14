import type { ButtonHTMLAttributes } from 'react'

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
}

/** A button styled by variant and size through tokens (see `.btn` in components.css). */
export function Button({ variant = 'secondary', size = 'md', className, type = 'button', ...rest }: ButtonProps) {
  return <button {...rest} type={type} data-variant={variant} data-size={size} className={`btn${className ? ' ' + className : ''}`} />
}
