import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type {
  Paginated,
  SantaGift,
  SantaHistoryEvent,
  SantaStartPayload,
  SantaState,
  SantaUpdatePayload,
} from '@/lib/api-types'

export const santaKey = ['santa'] as const
const historyKey = [...santaKey, 'history'] as const

/** DRF returns absolute `next` URLs; keep only the path so the dev proxy/same origin is used. */
function relative(url: string): string {
  const parsed = new URL(url, window.location.origin)
  return parsed.pathname + parsed.search
}

export function useSanta() {
  return useQuery({
    queryKey: santaKey,
    queryFn: () => apiFetch<SantaState>('/api/secret-santa/'),
  })
}

export function useSantaHistory() {
  return useInfiniteQuery({
    queryKey: historyKey,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiFetch<Paginated<SantaHistoryEvent>>(pageParam ?? '/api/secret-santa/history/'),
    getNextPageParam: (last) => (last.next ? relative(last.next) : undefined),
  })
}

/** Start, edit and end all return the fresh state: show it, then refresh the history too. */
function useStateMutation<V>(request: (variables: V) => Promise<SantaState>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: request,
    onSuccess: (state) => {
      queryClient.setQueryData(santaKey, state)
      return invalidateHistory(queryClient)
    },
  })
}

const invalidateHistory = (queryClient: QueryClient) =>
  queryClient.invalidateQueries({ queryKey: historyKey })

export function useStartSanta() {
  return useStateMutation((payload: SantaStartPayload) =>
    apiFetch<SantaState>('/api/secret-santa/', { method: 'POST', json: payload }),
  )
}

export function useUpdateSanta() {
  return useStateMutation((payload: SantaUpdatePayload) =>
    apiFetch<SantaState>('/api/secret-santa/', { method: 'PATCH', json: payload }),
  )
}

export function useEndSanta() {
  return useStateMutation(() => apiFetch<SantaState>('/api/secret-santa/', { method: 'DELETE' }))
}

export function useSetGiftNote() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, note }: { id: number; note: string }) =>
      apiFetch<SantaGift>(`/api/secret-santa/gifts/${id}/`, { method: 'PATCH', json: { note } }),
    onSuccess: () => invalidateHistory(queryClient),
  })
}
