import { debtKind } from '@/features/ledger/kinds/debt'
import { expenseKind } from '@/features/ledger/kinds/expense'
import { paymentKind } from '@/features/ledger/kinds/payment'
import type { LedgerKindUI } from '@/features/ledger/kinds/types'
import type { LedgerEntry, LedgerKindKey } from '@/lib/api-types'

type EntryOf<K extends LedgerKindKey> = Extract<LedgerEntry, { kind: K }>

const KINDS: { [K in LedgerKindKey]: LedgerKindUI<EntryOf<K>> } = {
  expense: expenseKind,
  debt: debtKind,
  payment: paymentKind,
}

/**
 * The UI for an entry's kind, typed for that entry. The cast is safe because `KINDS` is keyed
 * by `kind`: TypeScript just can't follow a union through a lookup.
 */
export function kindOf<E extends LedgerEntry>(entry: E): LedgerKindUI<E> {
  return KINDS[entry.kind] as unknown as LedgerKindUI<E>
}
