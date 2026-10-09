import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'
import type {
  ExpenseInput,
  LedgerBalances,
  LedgerEntry,
  LedgerEntryInput,
  LedgerMeta,
  LedgerRate,
  Paginated,
} from '@/lib/api-types'

export const ledgerKey = ['ledger'] as const
const entryKey = (id: number) => [...ledgerKey, 'entry', id] as const

const ENTRIES = '/api/ledger/entries/'

/** DRF returns absolute `next` URLs; keep only the path so the dev proxy/same origin is used. */
function relative(url: string): string {
  const parsed = new URL(url, window.location.origin)
  return parsed.pathname + parsed.search
}

/** The group's feed, newest first, loaded page by page. */
export function useFeed() {
  return useInfiniteQuery({
    queryKey: [...ledgerKey, 'feed'],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => apiFetch<Paginated<LedgerEntry>>(pageParam ?? ENTRIES),
    getNextPageParam: (last) => (last.next ? relative(last.next) : undefined),
  })
}

/** What a pact's settlement put in the ledger (few, so unpaged). */
export function usePactEntries(pactId: number) {
  return useQuery({
    queryKey: [...ledgerKey, 'pact', pactId],
    queryFn: () => apiFetch<LedgerEntry[]>(`${ENTRIES}?source_type=pact&source_id=${pactId}`),
  })
}

/** One entry; `null` skips loading (e.g. the form for a new expense). */
export function useEntry(id: number | null) {
  return useQuery({
    queryKey: entryKey(id ?? 0),
    queryFn: () => apiFetch<LedgerEntry>(`${ENTRIES}${id}/`),
    enabled: id !== null,
  })
}

export function useBalances() {
  return useQuery({
    queryKey: [...ledgerKey, 'balances'],
    queryFn: () => apiFetch<LedgerBalances>('/api/ledger/balances/'),
  })
}

/** Currencies (with their minor-unit digits) and categories the forms offer. */
export function useLedgerMeta() {
  return useQuery({
    queryKey: [...ledgerKey, 'meta'],
    queryFn: () => apiFetch<LedgerMeta>('/api/ledger/meta/'),
  })
}

/** The rate an expense in `currency` on `date` will use; asking also warms the server's cache. */
export function useRate(currency: string, date: string, enabled: boolean) {
  return useQuery({
    queryKey: [...ledgerKey, 'rate', currency, date],
    queryFn: () => apiFetch<LedgerRate>(`/api/ledger/rates/?currency=${currency}&date=${date}`),
    enabled,
    retry: false,
  })
}

/** Every change answers with the fresh entry: show it at once, then refresh everything else. */
function useEntryMutation<V>(request: (variables: V) => Promise<LedgerEntry>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: request,
    onSuccess: (entry) => {
      queryClient.setQueryData(entryKey(entry.id), entry)
      return queryClient.invalidateQueries({ queryKey: ledgerKey })
    },
  })
}

export function useCreateEntry() {
  return useEntryMutation((input: LedgerEntryInput) =>
    apiFetch<LedgerEntry>(ENTRIES, { method: 'POST', json: input }),
  )
}

/** Replace an expense; `version` is the one being edited (a 409 means somebody saved first). */
export function useUpdateExpense(id: number) {
  return useEntryMutation((v: { input: ExpenseInput; version: number }) =>
    apiFetch<LedgerEntry>(`${ENTRIES}${id}/`, {
      method: 'PUT',
      json: { ...v.input, version: v.version },
    }),
  )
}

/** Confirm or reject a payment (its receiver), or cancel an entry (whoever the server allows). */
export type EntryAction = 'confirm' | 'reject' | 'cancel'

export function useEntryAction() {
  return useEntryMutation((v: { id: number; action: EntryAction }) =>
    apiFetch<LedgerEntry>(`${ENTRIES}${v.id}/${v.action}/`, { method: 'POST' }),
  )
}

export function useAddAttachment(id: number) {
  return useEntryMutation((image: File) => {
    const form = new FormData()
    form.append('image', image)
    return apiFetch<LedgerEntry>(`${ENTRIES}${id}/attachments/`, { method: 'POST', form })
  })
}

export function useDeleteAttachment(id: number) {
  return useEntryMutation((attachmentId: number) =>
    apiFetch<LedgerEntry>(`${ENTRIES}${id}/attachments/${attachmentId}/`, { method: 'DELETE' }),
  )
}
