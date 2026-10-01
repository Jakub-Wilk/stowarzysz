import { Outlet } from 'react-router'

import { AppHeader } from '@/components/layout/AppHeader'
import { PageTransition } from '@/components/motion/PageTransition'

/** Full-screen pages reached from a tab (no tab bar, normal scrolling). */
export function SubPageLayout() {
  return (
    <div className="mx-auto flex min-h-svh max-w-2xl flex-col">
      <AppHeader />
      <main className="flex-1 p-4">
        <PageTransition>
          <Outlet />
        </PageTransition>
      </main>
    </div>
  )
}
