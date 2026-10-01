import { NavLink } from 'react-router'

import { tabs } from '@/components/layout/tabs'
import { cn } from '@/lib/utils'

export function BottomTabs() {
  return (
    <nav
      aria-label="Main"
      className="sticky bottom-0 z-10 border-t bg-card/90 pb-[env(safe-area-inset-bottom)] backdrop-blur"
    >
      <ul className="flex h-14">
        {tabs.map(({ path, label, icon: Icon }) => (
          <li key={path} className="flex-1">
            <NavLink
              to={path}
              aria-label={label}
              title={label}
              className={({ isActive }) =>
                cn(
                  'flex h-full items-center justify-center text-muted-foreground transition-colors hover:text-foreground',
                  isActive && 'text-primary',
                )
              }
            >
              {({ isActive }) => (
                <Icon
                  className={cn('size-6 transition-opacity', !isActive && 'opacity-65')}
                  aria-hidden
                />
              )}
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  )
}
