import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { meQueryKey } from '@/features/auth/hooks'
import { apiFetch } from '@/lib/api'
import type { ActivationLink, ManagedUser, ManagedUserPayload } from '@/lib/api-types'

const usersKey = ['admin', 'users'] as const

export function useManagedUsers() {
  return useQuery({
    queryKey: usersKey,
    queryFn: () => apiFetch<ManagedUser[]>('/api/auth/users/'),
  })
}

export function useManagedUser(id: number) {
  return useQuery({
    queryKey: [...usersKey, id],
    queryFn: () => apiFetch<ManagedUser>(`/api/auth/users/${id}/`),
  })
}

export function useCreateUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: ManagedUserPayload) =>
      apiFetch<ManagedUser>('/api/auth/users/', { method: 'POST', json: payload }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: usersKey }),
  })
}

export function useUpdateUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, payload }: { id: number; payload: Partial<ManagedUserPayload> }) =>
      apiFetch<ManagedUser>(`/api/auth/users/${id}/`, { method: 'PATCH', json: payload }),
    onSuccess: async () => {
      // The edited account may be the signed-in one (name shown in the header).
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: usersKey }),
        queryClient.invalidateQueries({ queryKey: meQueryKey }),
      ])
    },
  })
}

export function useDeleteUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => apiFetch<void>(`/api/auth/users/${id}/`, { method: 'DELETE' }),
    onSuccess: (_data, id) => {
      queryClient.removeQueries({ queryKey: [...usersKey, id] })
      return queryClient.invalidateQueries({ queryKey: usersKey })
    },
  })
}

/** Issues a fresh one-time link; any earlier unused link for the user stops working. */
export function useIssueActivationLink() {
  return useMutation({
    mutationFn: (id: number) =>
      apiFetch<ActivationLink>(`/api/auth/users/${id}/activation-link/`, { method: 'POST' }),
  })
}

function useAvatarMutation<V>(request: (variables: V) => Promise<ManagedUser>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: request,
    onSuccess: async () => {
      // The picture shows in the list, the login screen and (if it's theirs) the header.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: usersKey }),
        queryClient.invalidateQueries({ queryKey: meQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['login-users'] }),
      ])
    },
  })
}

export function useUploadAvatar() {
  return useAvatarMutation(({ id, file }: { id: number; file: File }) => {
    const form = new FormData()
    form.append('avatar', file)
    return apiFetch<ManagedUser>(`/api/auth/users/${id}/avatar/`, { method: 'PUT', form })
  })
}

export function useRemoveAvatar() {
  return useAvatarMutation((id: number) =>
    apiFetch<ManagedUser>(`/api/auth/users/${id}/avatar/`, { method: 'DELETE' }),
  )
}
