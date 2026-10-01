import { Navigate, Route, Routes } from 'react-router'

import { AppLayout } from '@/components/layout/AppLayout'
import { tabs } from '@/components/layout/tabs'
import { PublicOnly, RequireAuth } from '@/features/auth/RouteGuards'
import { LoginPage } from '@/pages/LoginPage'
import { MetalDefs } from '@/themes/MetalDefs'

export default function App() {
  return (
    <>
      <MetalDefs />
      <Routes>
        <Route element={<PublicOnly />}>
          <Route path="login" element={<LoginPage />} />
        </Route>
        <Route element={<RequireAuth />}>
          <Route index element={<Navigate to={tabs[0].path} replace />} />
          <Route element={<AppLayout />}>
            {tabs.map((t) => (
              <Route key={t.path} path={t.path} element={null} />
            ))}
          </Route>
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </>
  )
}
