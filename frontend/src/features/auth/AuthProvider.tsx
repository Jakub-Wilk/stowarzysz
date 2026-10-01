import { useQueryClient } from '@tanstack/react-query'
import { useEffect, type ReactNode } from 'react'

import { hasSession, subscribeSession } from '@/lib/auth-tokens'

/** Drops all cached server state when the session ends (logout, expiry, other tab). */
export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()

  useEffect(
    () =>
      subscribeSession(() => {
        if (!hasSession()) queryClient.clear()
      }),
    [queryClient],
  )

  return children
}
