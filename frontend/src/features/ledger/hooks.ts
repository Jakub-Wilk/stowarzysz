import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type { LedgerBalances, LedgerEntry } from '@/lib/api-types'

export const ledgerKey = ['ledger'] as const

export function useBalances() {
  return useQuery({
    queryKey: [...ledgerKey, 'balances'],
    queryFn: () => apiFetch<LedgerBalances>('/api/ledger/balances/'),
  })
}

/** Every entry in the group, newest first. */
export function useEntries() {
  return useQuery({
    queryKey: [...ledgerKey, 'entries'],
    queryFn: () => apiFetch<LedgerEntry[]>('/api/ledger/entries/'),
  })
}

export interface PaymentRequest {
  toUserId: number
  /** Grosze. */
  amount: number
  note: string
}

/** Pay somebody off. It only counts once they confirm it. */
export function useCreatePayment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (v: PaymentRequest) =>
      apiFetch<LedgerEntry>('/api/ledger/payments/', {
        method: 'POST',
        json: { to_user_id: v.toUserId, amount: v.amount, note: v.note },
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
  })
}

/** The receiver confirms or rejects a payment; the payer can cancel one nobody answered yet. */
export type PaymentAction = 'confirm' | 'reject' | 'cancel'

export function usePaymentAction() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (v: { id: number; action: PaymentAction }) =>
      apiFetch<LedgerEntry>(`/api/ledger/entries/${v.id}/${v.action}/`, { method: 'POST' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
  })
}
