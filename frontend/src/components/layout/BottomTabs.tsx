import { NavLink, useLocation } from 'react-router'

import { tabs } from '@/components/layout/tabs'
import { cn } from '@/lib/utils'

export function BottomTabs() {
  const { pathname } = useLocation()
  const activeIndex = Math.max(
    tabs.findIndex((t) => t.path === pathname),
    0,
  )

  return (
    <nav
      aria-label="Main"
      className="sticky bottom-0 z-10 border-t bg-card/90 pb-[env(safe-area-inset-bottom)] backdrop-blur"
    >
      <ul className="relative flex h-[4.5rem]">
        <span
          aria-hidden
          className="absolute top-0 h-0.5 rounded-full bg-primary transition-transform duration-(--duration-slow) ease-(--ease-spring)"
          style={{
            width: `${100 / tabs.length}%`,
            transform: `translateX(${activeIndex * 100}%)`,
          }}
        />
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
                  className={cn(
                    'size-8 transition-all duration-(--duration-base) ease-(--ease-spring)',
                    isActive ? 'scale-110' : 'opacity-65',
                  )}
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
