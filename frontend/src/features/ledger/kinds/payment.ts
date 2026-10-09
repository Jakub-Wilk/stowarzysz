import { ArrowRightLeft } from 'lucide-react'

import { who } from '@/features/ledger/format'
import { isGoods, partiesOf } from '@/features/ledger/kinds/helpers'
import { PaymentDetail } from '@/features/ledger/kinds/PaymentDetail'
import type { LedgerKindUI } from '@/features/ledger/kinds/types'
import type { PaymentEntry } from '@/lib/api-types'

export const paymentKind: LedgerKindUI<PaymentEntry> = {
  key: 'payment',
  label: 'Spłata',
  icon: ArrowRightLeft,
  settles: true,
  statusLabel: (entry) => {
    const masculine = isGoods(entry) // zwrot vs wpłata
    switch (entry.status) {
      case 'pending':
        return 'Czeka na potwierdzenie'
      case 'confirmed':
        return masculine ? 'Potwierdzony' : 'Potwierdzona'
      case 'rejected':
        return masculine ? 'Odrzucony' : 'Odrzucona'
      case 'cancelled':
        return masculine ? 'Wycofany' : 'Wycofana'
    }
  },
  summary: (entry, myId) => {
    const { payer, receiver } = partiesOf(entry)
    return `${who(payer, myId)} → ${who(receiver, myId)}`
  },
  actionLabels: (entry) => ({
    confirm: isGoods(entry) ? 'Potwierdzam zwrot' : 'Potwierdzam wpłatę',
    reject: 'Nie dostałem(-am)',
    cancel: isGoods(entry) ? 'Wycofaj zwrot' : 'Wycofaj wpłatę',
  }),
  Detail: PaymentDetail,
}
