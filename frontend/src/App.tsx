import { Navigate, Route, Routes } from 'react-router'

import { AppLayout } from '@/components/layout/AppLayout'
import { ManageLayout } from '@/components/layout/ManageLayout'
import { tabs } from '@/components/layout/tabs'
import { PublicOnly, RequireAuth, RequireSuperuser } from '@/features/auth/RouteGuards'
import { ActivatePage } from '@/pages/ActivatePage'
import { LoginPage } from '@/pages/LoginPage'
import { ManageIndexPage } from '@/pages/manage/ManageIndexPage'
import { UserFormPage } from '@/pages/manage/UserFormPage'
import { UsersPage } from '@/pages/manage/UsersPage'
import { MetalDefs } from '@/themes/MetalDefs'

export default function App() {
  return (
    <>
      <MetalDefs />
      <Routes>
        <Route element={<PublicOnly />}>
          <Route path="login" element={<LoginPage />} />
        </Route>
        {/* Reachable signed in or out: following a link switches to the activated account. */}
        <Route path="activate/:token" element={<ActivatePage />} />
        <Route element={<RequireAuth />}>
          <Route index element={<Navigate to={tabs[0].path} replace />} />
          <Route element={<AppLayout />}>
            {tabs.map((t) => (
              <Route key={t.path} path={t.path} element={null} />
            ))}
          </Route>
          <Route element={<RequireSuperuser />}>
            <Route path="manage" element={<ManageLayout />}>
              <Route index element={<ManageIndexPage />} />
              <Route path="users" element={<UsersPage />} />
              <Route path="users/new" element={<UserFormPage />} />
              <Route path="users/:id" element={<UserFormPage />} />
            </Route>
          </Route>
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </>
  )
}
