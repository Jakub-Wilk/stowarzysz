import { Navigate, Route, Routes } from 'react-router'

import { AppLayout } from '@/components/layout/AppLayout'
import { tabs } from '@/components/layout/tabs'

export default function App() {
  return (
    <Routes>
      <Route index element={<Navigate to={tabs[0].path} replace />} />
      <Route element={<AppLayout />}>
        {tabs.map((t) => (
          <Route key={t.path} path={t.path} element={null} />
        ))}
      </Route>
    </Routes>
  )
}
