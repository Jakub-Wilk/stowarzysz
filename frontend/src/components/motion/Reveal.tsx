import type { CSSProperties, ReactNode } from 'react'

import { cn } from '@/lib/utils'

/** Fades a single block up on mount. `index` offsets the delay when revealing several in a row. */
export function Reveal({
  children,
  index = 0,
  className,
}: {
  children: ReactNode
  index?: number
  className?: string
}) {
  return (
    <div
      className={cn('stagger animate-fade-up', className)}
      style={{ '--i': index } as CSSProperties}
    >
      {children}
    </div>
  )
}
