import { Outlet } from 'react-router'

import { AppHeader } from '@/components/layout/AppHeader'

/** Full-screen pages reached from a tab (no tab bar, normal scrolling). */
export function SubPageLayout() {
  return (
    <div className="mx-auto flex min-h-svh max-w-2xl flex-col">
      <AppHeader />
      <main className="flex-1 p-4">
        <Outlet />
      </main>
    </div>
  )
}
