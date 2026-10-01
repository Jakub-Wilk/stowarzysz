import { Navigate, Outlet } from 'react-router'

import { useHasSession, useMe } from '@/features/auth/hooks'

function FullScreenMessage({ children }: { children: string }) {
  return <div className="flex min-h-svh items-center justify-center text-sm">{children}</div>
}

/** Renders child routes only for a signed-in user; everyone else goes to /login. */
export function RequireAuth() {
  const hasSession = useHasSession()
  const me = useMe()

  if (!hasSession) return <Navigate to="/login" replace />
  if (me.isPending) return <FullScreenMessage>Loading…</FullScreenMessage>
  if (me.isError) {
    // A rejected refresh token clears the session (handled above); anything else is transient.
    return <FullScreenMessage>Can't reach the server. Please try again.</FullScreenMessage>
  }
  return <Outlet />
}

/** Keeps signed-in users away from the login screen. */
export function PublicOnly() {
  return useHasSession() ? <Navigate to="/" replace /> : <Outlet />
}

/** Child routes are for superusers only; use inside RequireAuth (needs the loaded profile). */
export function RequireSuperuser() {
  const { data: me } = useMe()
  return me?.is_superuser ? <Outlet /> : <Navigate to="/" replace />
}
