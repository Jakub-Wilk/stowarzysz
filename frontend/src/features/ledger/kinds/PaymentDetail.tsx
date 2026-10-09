import { ArrowRight } from 'lucide-react'

import { UserAvatar } from '@/features/auth/UserAvatar'
import { who } from '@/features/ledger/format'
import { isGoods, partiesOf } from '@/features/ledger/kinds/helpers'
import type { PaymentEntry } from '@/lib/api-types'

export function PaymentDetail({ entry, myId }: { entry: PaymentEntry; myId: number | undefined }) {
  const { payer, receiver } = partiesOf(entry)
  return (
    <section className="flex flex-col gap-3 rounded-lg border bg-card px-4 py-4">
      <div className="flex items-center justify-center gap-4">
        <span className="flex flex-col items-center gap-1">
          <UserAvatar username={payer.username} src={payer.avatar_url} size="lg" />
          <span className="text-base font-medium">{who(payer, myId)}</span>
        </span>
        <ArrowRight className="size-6 text-muted-foreground" aria-label="dla" />
        <span className="flex flex-col items-center gap-1">
          <UserAvatar username={receiver.username} src={receiver.avatar_url} size="lg" />
          <span className="text-base font-medium">{who(receiver, myId)}</span>
        </span>
      </div>
      {entry.status === 'pending' && (
        <p className="text-center text-sm text-muted-foreground">
          {isGoods(entry) ? 'Zwrot' : 'Wpłata'} liczy się dopiero, gdy {receiver.username}{' '}
          {isGoods(entry) ? 'go' : 'ją'} potwierdzi.
        </p>
      )}
    </section>
  )
}
