import { ChevronRight } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { who } from '@/features/ledger/format'
import { useBalances } from '@/features/ledger/hooks'
import { ListSkeleton, LoadError } from '@/features/ledger/LoadStates'
import { PayDialog } from '@/features/ledger/PayDialog'
import { toneClasses } from '@/features/voting/tone'
import type { LedgerMemberBalance, LedgerPendingPayment, LedgerTransfer } from '@/lib/api-types'
import { formatDebt } from '@/lib/money'
import { cn } from '@/lib/utils'

function Heading({ children }: { children: string }) {
  return <h3 className="mt-6 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

/** Bars compare like with like: all money together, each kind of item on its own. */
const unitOf = (row: { item: string }) => row.item.toLowerCase()

/** A bar growing right (others owe them) or left (they owe others) from a centre line. */
function MemberRow({
  row,
  scale,
  isMe,
}: {
  row: LedgerMemberBalance
  scale: number
  isMe: boolean
}) {
  const tone = toneClasses(row.net > 0 ? 'positive' : row.net < 0 ? 'negative' : 'neutral')
  const pct = scale > 0 ? (Math.abs(row.net) / scale) * 50 : 0
  const amount = formatDebt({ amount: row.net, item: row.item })
  return (
    <div
      className={cn(
        'flex flex-col gap-2 rounded-lg border bg-card p-4',
        isMe && 'border-primary/50',
      )}
    >
      <div className="flex items-center gap-4">
        <UserAvatar username={row.user.username} src={row.user.avatar_url} size="md" />
        <span className="text-metal min-w-0 flex-1 truncate text-lg font-semibold">
          {row.user.username}
          {isMe && ' (Ty)'}
        </span>
        <span className={cn('text-xl font-bold tabular-nums', tone.text)}>
          {row.net > 0 ? '+' : ''}
          {amount}
        </span>
      </div>
      <div className="relative h-2 rounded-full bg-muted" role="img" aria-label={`Saldo ${amount}`}>
        <span aria-hidden className="absolute inset-y-0 left-1/2 w-px bg-foreground/30" />
        <span
          aria-hidden
          className={cn(
            'absolute inset-y-0 rounded-full',
            row.net >= 0 ? 'left-1/2 bg-positive' : 'right-1/2 bg-negative',
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  )
}

/** Who should pay whom to get even. If it is you, there is a button to do it. */
function SettlementRow({ transfer, myId }: { transfer: LedgerTransfer; myId: number | undefined }) {
  const [paying, setPaying] = useState(false)
  const mine = transfer.debtor.id === myId
  return (
    <li
      className={cn(
        'flex flex-col gap-3 rounded-lg border bg-card px-4 py-3',
        mine && 'border-primary/50',
      )}
    >
      <div className="flex items-center gap-3">
        <UserAvatar
          username={transfer.debtor.username}
          src={transfer.debtor.avatar_url}
          size="sm"
        />
        <span className="min-w-0 flex-1 text-base">
          <strong>{who(transfer.debtor, myId)}</strong> →{' '}
          <strong>{who(transfer.creditor, myId)}</strong>
        </span>
        <span className="text-lg font-bold tabular-nums">{formatDebt(transfer)}</span>
      </div>
      {mine && (
        <>
          <Button variant="outline" onClick={() => setPaying(true)}>
            {transfer.item ? 'Oddaj' : 'Zapłać'} {transfer.creditor.username}
          </Button>
          <PayDialog transfer={transfer} open={paying} onOpenChange={setPaying} />
        </>
      )}
    </li>
  )
}

function PendingRow({
  payment,
  myId,
}: {
  payment: LedgerPendingPayment
  myId: number | undefined
}) {
  return (
    <li>
      <Link
        to={`/ledger/${payment.entry_id}`}
        className="flex items-center gap-3 rounded-lg border border-dashed bg-card px-4 py-3 hover:bg-accent"
      >
        <span className="min-w-0 flex-1 text-base">
          <strong>{who(payment.debtor, myId)}</strong> →{' '}
          <strong>{who(payment.creditor, myId)}</strong>
          <span className="block text-sm text-muted-foreground">
            czeka, aż{' '}
            {payment.creditor.id === myId
              ? 'potwierdzisz'
              : `${payment.creditor.username} potwierdzi`}
          </span>
        </span>
        <span className="text-lg font-bold tabular-nums">{formatDebt(payment)}</span>
        <ChevronRight className="size-5 shrink-0" aria-hidden />
      </Link>
    </li>
  )
}

/** The Bilans: everyone's total, then the fewest transfers that make everybody even. */
export function BalanceView({ myId }: { myId: number | undefined }) {
  const balances = useBalances()
  if (balances.isPending) return <ListSkeleton label="Ładowanie bilansu" />
  if (balances.isError) return <LoadError what="bilansu" onRetry={() => void balances.refetch()} />
  const { members, settlements, pending } = balances.data
  if (members.length === 0 && settlements.length === 0 && pending.length === 0) {
    return (
      <p className="mt-10 text-center text-base text-muted-foreground">
        Wszystko rozliczone. Nikt nikomu nic nie jest winien.
      </p>
    )
  }
  const scales = new Map<string, number>()
  for (const m of members) {
    scales.set(unitOf(m), Math.max(scales.get(unitOf(m)) ?? 0, Math.abs(m.net)))
  }
  return (
    <>
      {members.length > 0 && (
        <section>
          <Heading>Saldo każdego</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {members.map((row) => (
              <MemberRow
                key={`${row.user.id}-${unitOf(row)}`}
                row={row}
                scale={scales.get(unitOf(row)) ?? 0}
                isMe={row.user.id === myId}
              />
            ))}
          </Stagger>
        </section>
      )}
      {settlements.length > 0 && (
        <section>
          <Heading>Kto komu ile</Heading>
          <ul className="flex flex-col gap-2">
            {settlements.map((t) => (
              <SettlementRow
                key={`${t.debtor.id}-${t.creditor.id}-${unitOf(t)}`}
                transfer={t}
                myId={myId}
              />
            ))}
          </ul>
        </section>
      )}
      {pending.length > 0 && (
        <section>
          <Heading>Czeka na potwierdzenie</Heading>
          <ul className="flex flex-col gap-2">
            {pending.map((p) => (
              <PendingRow key={p.entry_id} payment={p} myId={myId} />
            ))}
          </ul>
        </section>
      )}
    </>
  )
}
