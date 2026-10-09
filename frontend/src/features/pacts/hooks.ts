import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type {
  PactCreatePayload,
  PactDetail,
  PactListItem,
  PactProposal,
  PactRespondPayload,
  PactStats,
  PactTerms,
} from '@/lib/api-types'

export const pactsKey = ['pacts'] as const
const detailKey = (id: number) => [...pactsKey, 'detail', id] as const

/** Every pact (members can read them all); the screens split them into running and archived. */
export function usePacts() {
  return useQuery({
    queryKey: [...pactsKey, 'list'],
    queryFn: () => apiFetch<PactListItem[]>('/api/pacts/'),
  })
}

export function usePact(id: number) {
  return useQuery({
    queryKey: detailKey(id),
    queryFn: () => apiFetch<PactDetail>(`/api/pacts/${id}/`),
  })
}

export function usePactStats() {
  return useQuery({
    queryKey: [...pactsKey, 'stats'],
    queryFn: () => apiFetch<PactStats[]>('/api/pacts/stats/'),
  })
}

/**
 * Most actions answer with the fresh pact: show it at once, then refresh everything else. A
 * settled claim also creates debts, so the ledger is refreshed too.
 */
function usePactMutation<V>(request: (variables: V) => Promise<PactDetail>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: request,
    onSuccess: (pact) => {
      queryClient.setQueryData(detailKey(pact.id), pact)
      void queryClient.invalidateQueries({ queryKey: ['ledger'] })
      return queryClient.invalidateQueries({ queryKey: pactsKey })
    },
  })
}

export function useCreatePact() {
  return usePactMutation((payload: PactCreatePayload) =>
    apiFetch<PactDetail>('/api/pacts/', { method: 'POST', json: payload }),
  )
}

export function useRespond(id: number) {
  return usePactMutation((payload: PactRespondPayload) =>
    apiFetch<PactDetail>(`/api/pacts/${id}/respond/`, { method: 'POST', json: payload }),
  )
}

export function useCancelPact(id: number) {
  return usePactMutation(() => apiFetch<PactDetail>(`/api/pacts/${id}/cancel/`, { method: 'POST' }))
}

export function useRequestJoin(id: number) {
  return usePactMutation((terms: PactTerms) =>
    apiFetch<PactDetail>(`/api/pacts/${id}/join/`, { method: 'POST', json: terms }),
  )
}

export function useWithdrawRequest(id: number) {
  return usePactMutation(() =>
    apiFetch<PactDetail>(`/api/pacts/${id}/withdraw/`, { method: 'POST' }),
  )
}

export function useDecideJoin(id: number) {
  return usePactMutation((v: { participantId: number; approve: boolean }) =>
    apiFetch<PactDetail>(`/api/pacts/${id}/participants/${v.participantId}/decide/`, {
      method: 'POST',
      json: { approve: v.approve },
    }),
  )
}

export function useProposeOutcome(id: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (v: { wagerId: number | null; result: Record<string, unknown> }) =>
      apiFetch<PactProposal>(`/api/pacts/${id}/outcome/`, {
        method: 'POST',
        json: { wager_id: v.wagerId, result: v.result },
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: pactsKey }),
  })
}

export type ClaimAction = 'confirm' | 'dispute' | 'escalate'

export function useClaimAction(id: number) {
  return usePactMutation((v: { proposalId: number; action: ClaimAction }) =>
    apiFetch<PactDetail>(`/api/pacts/${id}/outcome/${v.proposalId}/${v.action}/`, {
      method: 'POST',
    }),
  )
}

export function useAddAttachment(id: number) {
  return usePactMutation((v: { image: File; caption: string }) => {
    const form = new FormData()
    form.append('image', v.image)
    if (v.caption.trim()) form.append('caption', v.caption.trim())
    return apiFetch<PactDetail>(`/api/pacts/${id}/attachments/`, { method: 'POST', form })
  })
}

export function useDeleteAttachment(id: number) {
  return usePactMutation((attachmentId: number) =>
    apiFetch<PactDetail>(`/api/pacts/${id}/attachments/${attachmentId}/`, { method: 'DELETE' }),
  )
}
