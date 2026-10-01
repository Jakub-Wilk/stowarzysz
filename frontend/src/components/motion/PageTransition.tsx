import type { ReactNode } from 'react'
import { useLocation } from 'react-router'

import { cn } from '@/lib/utils'

/** Re-mounts (and so re-animates) its children on every route change. Use in layouts. */
export function PageTransition({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  const { pathname } = useLocation()
  return (
    <div key={pathname} className={cn('animate-fade-up', className)}>
      {children}
    </div>
  )
}
