import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useSyncExternalStore } from 'react'

import { apiFetch } from '@/lib/api'
import type {
  ActivationCompletePayload,
  ActivationValidateResult,
  LoginPayload,
  LoginUser,
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

/** Accounts shown on the login screen (public endpoint). */
export function useLoginUsers() {
  return useQuery({
    queryKey: ['login-users'],
    queryFn: () => apiFetch<LoginUser[]>('/api/auth/login-users/', { auth: false }),
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

/** Checks an activation link without consuming it (POST so the token stays out of URLs and logs). */
export function useValidateActivation(token: string) {
  return useQuery({
    queryKey: ['activation', token],
    queryFn: () =>
      apiFetch<ActivationValidateResult>('/api/auth/activation/validate/', {
        method: 'POST',
        json: { token },
        auth: false,
      }),
    retry: false,
    staleTime: Infinity,
  })
}

/** Sets the password; the user still has to sign in afterwards. */
export function useActivate() {
  return useMutation({
    mutationFn: (payload: ActivationCompletePayload) =>
      apiFetch<void>('/api/auth/activation/complete/', {
        method: 'POST',
        json: payload,
        auth: false,
      }),
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
