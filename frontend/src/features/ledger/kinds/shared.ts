import type { LucideIcon } from 'lucide-react'

import { WORDING } from '@/features/ledger/expense/wording'
import { who } from '@/features/ledger/format'
import { ExpenseDetail } from '@/features/ledger/kinds/ExpenseDetail'
import { peopleOf } from '@/features/ledger/kinds/helpers'
import type { LedgerKindUI } from '@/features/ledger/kinds/types'
import type { SharedEntry } from '@/lib/api-types'
import { PERSON_GENITIVE_FORMS, plural } from '@/lib/plural'

/** The UI of a kind with payers and items (an expense, or an income: the same with the sign
 * flipped), worded by `WORDING`. */
export function sharedKind<E extends SharedEntry>(
  key: E['kind'],
  icon: LucideIcon,
): LedgerKindUI<E> {
  const words = WORDING[key]
  return {
    key,
    label: words.noun,
    icon,
    statusLabel: (entry) => (entry.status === 'cancelled' ? 'Usunięty' : 'Zapisany'),
    summary: (entry, myId) => {
      const people = peopleOf(entry)
      const payers = entry.details.payers.flatMap((p) => people.get(p.user_id) ?? [])
      const by =
        payers.length !== 1
          ? `${words.paidMany}: ${payers.map((p) => who(p, myId)).join(', ')}`
          : payers[0].id === myId
            ? words.paidMe
            : `${words.paid} ${payers[0].username}`
      if (key === 'expense') return by
      const sharers = entry.breakdown.filter((row) => row.owed > 0).length
      return `${by} · dla ${sharers} ${plural(sharers, PERSON_GENITIVE_FORMS)}`
    },
    actionLabels: () => ({ confirm: '', reject: '', cancel: words.cancel }),
    cancelQuestion: words.cancelQuestion,
    Detail: ExpenseDetail,
  }
}
