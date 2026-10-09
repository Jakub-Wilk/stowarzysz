import type { PactKindUI } from '@/features/pacts/kinds/types'
import type { PactTerms } from '@/lib/api-types'
import { parseMoney } from '@/lib/money'

/** The text a person typed for their side and stake; `buildTerms` turns it into an API payload. */
export interface TermsState {
  side: string
  amount: string
  note: string
}

export const emptyTerms: TermsState = { side: '', amount: '', note: '' }

export type TermsResult = { terms: PactTerms; error: null } | { terms: null; error: string }

/** Validates what the kind needs (the server re-checks everything) and converts the amount. */
export function buildTerms(kind: PactKindUI, state: TermsState): TermsResult {
  const amountText = state.amount.trim()
  const amount = amountText ? parseMoney(amountText) : null
  if (amountText && amount === null) {
    return { terms: null, error: 'Podaj poprawną kwotę, np. 10 lub 12,50.' }
  }
  if (kind.sided && !state.side) return { terms: null, error: 'Wybierz stronę.' }
  if (kind.money === 'wager' && amount === null && !state.note.trim()) {
    return { terms: null, error: 'Podaj stawkę: kwotę albo opis (np. kolacja).' }
  }
  return {
    terms: {
      side: kind.sided ? state.side : undefined,
      stake_amount: kind.money === 'none' ? undefined : amount,
      stake_note: state.note.trim() || undefined,
    },
    error: null,
  }
}
