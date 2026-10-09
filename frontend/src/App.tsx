import { Navigate, Route, Routes } from 'react-router'

import { AppLayout } from '@/components/layout/AppLayout'
import { ManageLayout } from '@/components/layout/ManageLayout'
import { SubPageLayout } from '@/components/layout/SubPageLayout'
import { tabs } from '@/components/layout/tabs'
import { useHasSession } from '@/features/auth/hooks'
import { PublicOnly, RequireAuth, RequireSuperuser } from '@/features/auth/RouteGuards'
import { RealtimeShell } from '@/features/voting/realtime'
import { ActivatePage } from '@/pages/ActivatePage'
import { ExpenseEditorPage } from '@/pages/ExpenseEditorPage'
import { LandingPage } from '@/pages/LandingPage'
import { LedgerEntryPage } from '@/pages/LedgerEntryPage'
import { LoginPage } from '@/pages/LoginPage'
import { AvatarArchivePage } from '@/pages/manage/AvatarArchivePage'
import { ManageIndexPage } from '@/pages/manage/ManageIndexPage'
import { SantaManagePage } from '@/pages/manage/SantaManagePage'
import { UserFormPage } from '@/pages/manage/UserFormPage'
import { UsersPage } from '@/pages/manage/UsersPage'
import { PactDetailPage } from '@/pages/PactDetailPage'
import { PactNewPage } from '@/pages/PactNewPage'
import { VoteDetailPage } from '@/pages/VoteDetailPage'
import { VoteNewPage } from '@/pages/VoteNewPage'
import { MetalDefs } from '@/themes/MetalDefs'

/** Signed-in users go straight to the app; everyone else sees the landing page. */
function Home() {
  return useHasSession() ? <Navigate to={tabs[0].path} replace /> : <LandingPage />
}

export default function App() {
  return (
    <>
      <MetalDefs />
      <Routes>
        <Route index element={<Home />} />
        <Route element={<PublicOnly />}>
          <Route path="login" element={<LoginPage />} />
        </Route>
        {/* Reachable signed in or out: following a link switches to the activated account. */}
        <Route path="activate/:token" element={<ActivatePage />} />
        <Route element={<RequireAuth />}>
          <Route element={<RealtimeShell />}>
            <Route element={<AppLayout />}>
              {tabs.map((t) => (
                <Route key={t.path} path={t.path} element={null} />
              ))}
            </Route>
            <Route element={<SubPageLayout />}>
              <Route path="voting/new" element={<VoteNewPage />} />
              <Route path="voting/:id" element={<VoteDetailPage />} />
              <Route path="pacts/new" element={<PactNewPage />} />
              <Route path="pacts/:id" element={<PactDetailPage />} />
              <Route path="ledger/new" element={<ExpenseEditorPage />} />
              <Route path="ledger/:id" element={<LedgerEntryPage />} />
              <Route path="ledger/:id/edit" element={<ExpenseEditorPage />} />
            </Route>
            <Route element={<RequireSuperuser />}>
              <Route path="manage" element={<ManageLayout />}>
                <Route index element={<ManageIndexPage />} />
                <Route path="users" element={<UsersPage />} />
                <Route path="users/new" element={<UserFormPage />} />
                <Route path="users/:id" element={<UserFormPage />} />
                <Route path="secret-santa" element={<SantaManagePage />} />
                <Route path="avatar-archive" element={<AvatarArchivePage />} />
              </Route>
            </Route>
          </Route>
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </>
  )
}
