import { useLocation, useNavigate } from 'react-router'

import { AppHeader } from '@/components/layout/AppHeader'
import { BottomTabs } from '@/components/layout/BottomTabs'
import { SwipePager } from '@/components/layout/SwipePager'
import { tabs } from '@/components/layout/tabs'

export function AppLayout() {
  const { pathname } = useLocation()
  const navigate = useNavigate()
  const index = Math.max(
    tabs.findIndex((t) => t.path === pathname),
    0,
  )

  return (
    <div className="mx-auto flex h-svh max-w-2xl flex-col overflow-hidden">
      <AppHeader />
      <main className="flex min-h-0 flex-1 flex-col">
        <SwipePager index={index} onIndexChange={(i) => navigate(tabs[i].path)}>
          {tabs.map(({ path, page: Page }) => (
            <Page key={path} />
          ))}
        </SwipePager>
      </main>
      <BottomTabs />
    </div>
  )
}
