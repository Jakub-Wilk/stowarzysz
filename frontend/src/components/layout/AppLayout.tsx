import { useLocation, useNavigate } from 'react-router'

import { AppHeader } from '@/components/layout/AppHeader'
import { useBackToRefresh } from '@/components/layout/backButton'
import { BottomTabs } from '@/components/layout/BottomTabs'
import { SwipePager } from '@/components/layout/SwipePager'
import { tabs } from '@/components/layout/tabs'

export function AppLayout() {
  const { pathname } = useLocation()
  const navigate = useNavigate()
  useBackToRefresh()
  const index = Math.max(
    tabs.findIndex((t) => t.path === pathname),
    0,
  )

  return (
    <div className="mx-auto flex min-h-svh max-w-2xl flex-col overflow-x-clip">
      <AppHeader />
      <main className="flex flex-1 flex-col">
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
