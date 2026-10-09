import { Receipt } from 'lucide-react'

import { who } from '@/features/ledger/format'
import { ExpenseDetail } from '@/features/ledger/kinds/ExpenseDetail'
import { peopleOf } from '@/features/ledger/kinds/helpers'
import type { LedgerKindUI } from '@/features/ledger/kinds/types'
import type { ExpenseEntry } from '@/lib/api-types'

export const expenseKind: LedgerKindUI<ExpenseEntry> = {
  key: 'expense',
  label: 'Wydatek',
  icon: Receipt,
  statusLabel: (entry) => (entry.status === 'cancelled' ? 'Usunięty' : 'Zapisany'),
  summary: (entry, myId) => {
    const people = peopleOf(entry)
    const payers = entry.details.payers.flatMap((p) => people.get(p.user_id) ?? [])
    if (payers.length === 1) {
      return payers[0].id === myId ? 'zapłaciłeś(-aś) Ty' : `zapłacił(a) ${payers[0].username}`
    }
    return `zapłacili: ${payers.map((p) => who(p, myId)).join(', ')}`
  },
  actionLabels: () => ({ confirm: '', reject: '', cancel: 'Usuń wydatek' }),
  cancelQuestion: 'Usunąć ten wydatek? Przestanie się liczyć w bilansie.',
  Detail: ExpenseDetail,
}
