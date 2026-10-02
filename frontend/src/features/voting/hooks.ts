import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type {
  Ballot,
  Paginated,
  PollCreatePayload,
  PollDetail,
  PollListItem,
  ReactionEmoji,
  UserBrief,
} from '@/lib/api-types'

const pollsKey = ['polls'] as const
const detailKey = (id: number) => [...pollsKey, 'detail', id] as const

const listUrl = (status: string) => `/api/polls/?status=${status}`

/** DRF returns absolute `next` URLs; keep only the path so the dev proxy/same origin is used. */
function relative(url: string): string {
  const parsed = new URL(url, window.location.origin)
  return parsed.pathname + parsed.search
}

/** Active votes: few, so returned in one go. */
export function useOpenPolls() {
  return useQuery({
    queryKey: [...pollsKey, 'open'],
    queryFn: () => apiFetch<PollListItem[]>(listUrl('open')),
  })
}

/** History: newest first, loaded page by page. */
export function useClosedPolls() {
  return useInfiniteQuery({
    queryKey: [...pollsKey, 'closed'],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => apiFetch<Paginated<PollListItem>>(pageParam ?? listUrl('closed')),
    getNextPageParam: (last) => (last.next ? relative(last.next) : undefined),
  })
}

export function usePoll(id: number) {
  return useQuery({
    queryKey: detailKey(id),
    queryFn: () => apiFetch<PollDetail>(`/api/polls/${id}/`),
  })
}

export function usePeople() {
  return useQuery({
    queryKey: ['people'],
    queryFn: () => apiFetch<UserBrief[]>('/api/auth/people/'),
    staleTime: 60_000,
  })
}

/** Every action returns the fresh poll: show it immediately, then refresh the lists. */
function useRefreshingPollMutation<V>(request: (variables: V) => Promise<PollDetail>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: request,
    onSuccess: (poll) => {
      queryClient.setQueryData(detailKey(poll.id), poll)
      return invalidateLists(queryClient)
    },
  })
}

const invalidateLists = (queryClient: QueryClient) =>
  queryClient.invalidateQueries({ queryKey: pollsKey })

export function useCreatePoll() {
  return useRefreshingPollMutation((payload: PollCreatePayload) =>
    apiFetch<PollDetail>('/api/polls/', { method: 'POST', json: payload }),
  )
}

export function useCastBallot(id: number) {
  return useRefreshingPollMutation((ballot: Ballot) =>
    apiFetch<PollDetail>(`/api/polls/${id}/ballot/`, { method: 'PUT', json: { ballot } }),
  )
}

export function useVeto(id: number) {
  return useRefreshingPollMutation(() =>
    apiFetch<PollDetail>(`/api/polls/${id}/veto/`, { method: 'POST' }),
  )
}

export function useClosePoll(id: number) {
  return useRefreshingPollMutation(() =>
    apiFetch<PollDetail>(`/api/polls/${id}/close/`, { method: 'POST' }),
  )
}

export function useSendReaction(id: number) {
  return useMutation({
    mutationFn: (emoji: ReactionEmoji) =>
      apiFetch<void>(`/api/polls/${id}/reactions/`, { method: 'POST', json: { emoji } }),
  })
}
