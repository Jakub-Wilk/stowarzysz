import { HandCoins } from 'lucide-react'

import { who } from '@/features/ledger/format'
import { DebtDetail } from '@/features/ledger/kinds/DebtDetail'
import type { LedgerKindUI } from '@/features/ledger/kinds/types'
import type { DebtEntry } from '@/lib/api-types'

export const debtKind: LedgerKindUI<DebtEntry> = {
  key: 'debt',
  label: 'Dług',
  icon: HandCoins,
  statusLabel: (entry) => (entry.status === 'cancelled' ? 'Usunięty' : 'Zapisany'),
  summary: (entry, myId) => {
    const from = entry.source_type === 'pact' ? 'z zakładu: ' : ''
    const pairs = entry.obligations.map((o) => `${who(o.debtor, myId)} → ${who(o.creditor, myId)}`)
    return from + pairs.join(', ')
  },
  actionLabels: () => ({ confirm: '', reject: '', cancel: 'Usuń wpis' }),
  cancelQuestion: 'Usunąć ten wpis? Przestanie się liczyć w bilansie.',
  Detail: DebtDetail,
}
