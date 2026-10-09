import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type { LedgerBalance, LedgerEntry } from '@/lib/api-types'

export const ledgerKey = ['ledger'] as const

export function useBalances() {
  return useQuery({
    queryKey: [...ledgerKey, 'balances'],
    queryFn: () => apiFetch<LedgerBalance[]>('/api/ledger/balances/'),
  })
}

export function useEntries() {
  return useQuery({
    queryKey: [...ledgerKey, 'entries'],
    queryFn: () => apiFetch<LedgerEntry[]>('/api/ledger/entries/'),
  })
}

export type EntryAction = 'paid' | 'confirm'

/** The debtor marks a debt paid ("paid"); the creditor then closes it ("confirm"). */
export function useEntryAction() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (v: { id: number; action: EntryAction }) =>
      apiFetch<LedgerEntry>(`/api/ledger/entries/${v.id}/${v.action}/`, { method: 'POST' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
  })
}
