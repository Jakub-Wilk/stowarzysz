import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useSyncExternalStore } from 'react'

import { apiFetch } from '@/lib/api'
import type {
  ActivationCompletePayload,
  ActivationValidateResult,
  LoginPayload,
  Me,
  TokenPair,
} from '@/lib/api-types'
import {
  clearSession,
  getRefreshToken,
  hasSession,
  setTokens,
  subscribeSession,
} from '@/lib/auth-tokens'

export const meQueryKey = ['me'] as const

/** True while a refresh token is held; re-renders on login/logout, also from other tabs. */
export function useHasSession(): boolean {
  return useSyncExternalStore(subscribeSession, hasSession)
}

export function useMe() {
  const enabled = useHasSession()
  return useQuery({
    queryKey: meQueryKey,
    queryFn: () => apiFetch<Me>('/api/auth/me/'),
    enabled,
    staleTime: 60_000,
  })
}

export function useLogin() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: LoginPayload) =>
      apiFetch<TokenPair>('/api/auth/token/', { method: 'POST', json: payload, auth: false }),
    onSuccess: (pair) => {
      setTokens(pair)
      return queryClient.invalidateQueries({ queryKey: meQueryKey })
    },
  })
}

export function useLogout() {
  return useMutation({
    mutationFn: async () => {
      const refresh = getRefreshToken()
      if (refresh) {
        // Best effort: blacklist server-side, but always end the local session.
        await apiFetch('/api/auth/token/blacklist/', {
          method: 'POST',
          json: { refresh },
          auth: false,
        }).catch(() => undefined)
      }
    },
    onSettled: () => clearSession(), // AuthProvider clears the query cache
  })
}

export function useValidateActivation() {
  return useMutation({
    mutationFn: (token: string) =>
      apiFetch<ActivationValidateResult>('/api/auth/activation/validate/', {
        method: 'POST',
        json: { token },
        auth: false,
      }),
  })
}

/** Sets the password and signs the user in with the returned token pair. */
export function useActivate() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: ActivationCompletePayload) =>
      apiFetch<TokenPair>('/api/auth/activation/complete/', {
        method: 'POST',
        json: payload,
        auth: false,
      }),
    onSuccess: (pair) => {
      setTokens(pair)
      return queryClient.invalidateQueries({ queryKey: meQueryKey })
    },
  })
}

export function useAuth() {
  const isAuthenticated = useHasSession()
  const me = useMe()
  const login = useLogin()
  const logout = useLogout()
  return {
    user: me.data ?? null,
    isAuthenticated,
    isLoading: isAuthenticated && me.isPending,
    login: login.mutateAsync,
    logout: logout.mutateAsync,
  }
}
