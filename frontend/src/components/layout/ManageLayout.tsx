import { Outlet } from 'react-router'

import { AppHeader } from '@/components/layout/AppHeader'
import { PageTransition } from '@/components/motion/PageTransition'

/** Shell for the admin UI: same header, no tab bar, normal vertical scrolling. */
export function ManageLayout() {
  return (
    <div className="mx-auto flex min-h-svh max-w-2xl flex-col">
      <AppHeader section="Management" />
      <main className="flex-1 p-4">
        <PageTransition>
          <Outlet />
        </PageTransition>
      </main>
    </div>
  )
}
