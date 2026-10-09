import { useState } from 'react'
import { Link } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import {
  useBalances,
  useCreatePayment,
  useEntries,
  usePaymentAction,
} from '@/features/ledger/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import { dayGroup } from '@/features/voting/dates'
import { toneClasses } from '@/features/voting/tone'
import type {
  LedgerBalances,
  LedgerEntry,
  LedgerMemberBalance,
  LedgerPairBalance,
  Tone,
} from '@/lib/api-types'
import { formatMoney, parseMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

type Tab = 'entries' | 'balance'

const TABS: { key: Tab; label: string }[] = [
  { key: 'entries', label: 'Wpisy' },
  { key: 'balance', label: 'Bilans' },
]

function Heading({ children }: { children: string }) {
  return <h3 className="mt-6 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

const STATUS_TEXT: Record<LedgerEntry['status'], string> = {
  pending: 'Czeka na potwierdzenie',
  confirmed: 'Potwierdzona',
  rejected: 'Odrzucona',
  cancelled: 'Wycofana',
}

const STATUS_TONE: Record<LedgerEntry['status'], Tone> = {
  pending: 'neutral',
  confirmed: 'positive',
  rejected: 'negative',
  cancelled: 'neutral',
}

/** A debt is a plain fact; a payment shows its state and the buttons for whoever can act on it. */
function Entry({ entry, myId }: { entry: LedgerEntry; myId: number | undefined }) {
  const action = usePaymentAction()
  const isPayment = entry.kind === 'payment'
  const iAmDebtor = entry.debtor.id === myId
  const iAmCreditor = entry.creditor.id === myId
  const counts = entry.status === 'confirmed'
  const source =
    entry.source_type === 'pact' && entry.source_id !== null ? `/pacts/${entry.source_id}` : null
  const who = iAmDebtor ? 'Ty' : entry.debtor.username
  const whom = iAmCreditor ? 'Tobie' : entry.creditor.username
  const verb = isPayment
    ? iAmDebtor
      ? 'zapłaciłeś(-aś)'
      : 'zapłacił(a)'
    : iAmDebtor
      ? 'jesteś winien(-na)'
      : 'jest winien(-na)'

  return (
    <li
      className={cn(
        'flex flex-col gap-3 rounded-lg border bg-card p-4',
        !counts && 'opacity-70',
        (iAmDebtor || iAmCreditor) && counts && 'border-primary/50',
      )}
    >
      <div className="flex items-center gap-4">
        <UserAvatar username={entry.debtor.username} src={entry.debtor.avatar_url} size="md" />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-metal text-lg font-semibold break-words">
            {who} {verb} {whom}
          </span>
          <span className="truncate text-sm text-muted-foreground">
            {source ? (
              <Link to={source} className="underline">
                {entry.description}
              </Link>
            ) : (
              entry.description
            )}
          </span>
        </div>
        <span
          className={cn(
            'text-xl font-bold tabular-nums',
            isPayment && toneClasses(STATUS_TONE[entry.status]).text,
          )}
        >
          {formatMoney(entry.amount)}
        </span>
      </div>
      {isPayment && (
        <span
          className={cn(
            'self-start rounded-full px-2.5 py-0.5 text-xs font-bold uppercase',
            toneClasses(STATUS_TONE[entry.status]).chip,
          )}
        >
          {STATUS_TEXT[entry.status]}
        </span>
      )}
      {isPayment && entry.status === 'pending' && iAmCreditor && (
        <div className="flex gap-2">
          <Button
            className="flex-1"
            disabled={action.isPending}
            onClick={() => action.mutate({ id: entry.id, action: 'confirm' })}
          >
            Potwierdzam wpłatę
          </Button>
          <Button
            className="flex-1"
            variant="outline"
            disabled={action.isPending}
            onClick={() => action.mutate({ id: entry.id, action: 'reject' })}
          >
            Nie dostałem(-am)
          </Button>
        </div>
      )}
      {isPayment && entry.status === 'pending' && iAmDebtor && (
        <Button
          variant="outline"
          disabled={action.isPending}
          onClick={() => action.mutate({ id: entry.id, action: 'cancel' })}
        >
          Wycofaj wpłatę
        </Button>
      )}
      <MutationError error={action.error} />
    </li>
  )
}

/** Every entry in the group, newest first and filed under its day, like Tricount's list. */
function EntriesTab({ myId }: { myId: number | undefined }) {
  const entries = useEntries()
  if (entries.isPending) return <ListSkeleton label="Ładowanie wpisów" />
  if (entries.isError) return <LoadError label="wpisów" onRetry={() => void entries.refetch()} />
  if (entries.data.length === 0) {
    return (
      <p className="mt-10 text-center text-base text-muted-foreground">
        Nikt nikomu nic nie jest winien. Na razie.
      </p>
    )
  }
  const groups = new Map<string, LedgerEntry[]>()
  for (const entry of entries.data) {
    const label = dayGroup(entry.created_at)
    groups.set(label, [...(groups.get(label) ?? []), entry])
  }
  return (
    <>
      {[...groups].map(([label, rows]) => (
        <section key={label}>
          <Heading>{label}</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {rows.map((entry) => (
              <Entry key={entry.id} entry={entry} myId={myId} />
            ))}
          </Stagger>
        </section>
      ))}
    </>
  )
}

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
  return (
    <li
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
          {formatMoney(row.net)}
        </span>
      </div>
      <div
        className="relative h-2 rounded-full bg-muted"
        role="img"
        aria-label={`Saldo ${formatMoney(row.net)}`}
      >
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
    </li>
  )
}

/** Who owes whom. If it is me, there is a button to pay it off (the receiver confirms). */
function PairRow({
  pair,
  myId,
  pending,
}: {
  pair: LedgerPairBalance
  myId: number | undefined
  pending: number
}) {
  const [paying, setPaying] = useState(false)
  const iOwe = pair.debtor.id === myId
  return (
    <li className="flex flex-col gap-3 rounded-lg border bg-card px-4 py-3">
      <div className="flex items-center gap-3">
        <UserAvatar username={pair.debtor.username} src={pair.debtor.avatar_url} size="sm" />
        <span className="min-w-0 flex-1 text-base">
          <strong>{iOwe ? 'Ty' : pair.debtor.username}</strong>{' '}
          {iOwe ? 'jesteś winien(-na)' : 'jest winien(-na)'}{' '}
          <strong>{pair.creditor.id === myId ? 'Tobie' : pair.creditor.username}</strong>
        </span>
        <span className="text-lg font-bold tabular-nums">{formatMoney(pair.amount)}</span>
      </div>
      {pending > 0 && (
        <span className="text-sm text-muted-foreground">
          Wpłata {formatMoney(pending)} czeka na potwierdzenie przez {pair.creditor.username}.
        </span>
      )}
      {iOwe && (
        <>
          <Button variant="outline" onClick={() => setPaying(true)}>
            Zapłać {pair.creditor.username}
          </Button>
          <PayDialog pair={pair} open={paying} onOpenChange={setPaying} />
        </>
      )}
    </li>
  )
}

/** Start a payment: the amount (the whole debt to begin with) and an optional note. */
function PayDialog({
  pair,
  open,
  onOpenChange,
}: {
  pair: LedgerPairBalance
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const pay = useCreatePayment()
  const [amount, setAmount] = useState(() => String(pair.amount / 100).replace('.', ','))
  const [note, setNote] = useState('')
  const [localError, setLocalError] = useState<string | null>(null)

  const submit = () => {
    setLocalError(null)
    const grosze = parseMoney(amount)
    if (grosze === null) return setLocalError('Podaj poprawną kwotę, np. 10 lub 12,50.')
    pay.mutate(
      { toUserId: pair.creditor.id, amount: grosze, note },
      {
        onSuccess: () => {
          setNote('')
          onOpenChange(false)
        },
      },
    )
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setLocalError(null)
          pay.reset()
        }
        onOpenChange(next)
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Zapłać {pair.creditor.username}</DialogTitle>
          <DialogDescription>
            Wpłata zmniejszy dług dopiero wtedy, gdy {pair.creditor.username} ją potwierdzi.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <Label htmlFor={`pay-amount-${pair.creditor.id}`}>Kwota (zł)</Label>
          <Input
            id={`pay-amount-${pair.creditor.id}`}
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor={`pay-note-${pair.creditor.id}`}>Notatka (opcjonalnie)</Label>
          <Input
            id={`pay-note-${pair.creditor.id}`}
            maxLength={200}
            placeholder="np. BLIK, gotówka"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </div>
        {localError && (
          <span role="alert" className="text-base text-destructive">
            {localError}
          </span>
        )}
        <MutationError error={pay.error} />
        <DialogFooter>
          <Button disabled={pay.isPending} onClick={submit}>
            {pay.isPending ? 'Wysyłanie…' : 'Wyślij do potwierdzenia'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/** Totals per person, then who owes whom once each two people's debts are netted. */
function BalanceTab({ myId }: { myId: number | undefined }) {
  const balances = useBalances()
  const entries = useEntries()
  if (balances.isPending) return <ListSkeleton label="Ładowanie bilansu" />
  if (balances.isError) return <LoadError label="bilansu" onRetry={() => void balances.refetch()} />
  const { members, pairs }: LedgerBalances = balances.data
  if (members.length === 0) {
    return (
      <p className="mt-10 text-center text-base text-muted-foreground">
        Wszystko rozliczone. Nikt nikomu nic nie jest winien.
      </p>
    )
  }
  const scale = Math.max(...members.map((m) => Math.abs(m.net)))
  // my payments the receiver hasn't answered yet, so the row can say so instead of looking unpaid
  const pendingFrom = (pair: LedgerPairBalance): number =>
    (entries.data ?? [])
      .filter(
        (e) =>
          e.kind === 'payment' &&
          e.status === 'pending' &&
          e.debtor.id === pair.debtor.id &&
          e.creditor.id === pair.creditor.id,
      )
      .reduce((sum, e) => sum + e.amount, 0)
  return (
    <>
      <section>
        <Heading>Saldo każdego</Heading>
        <Stagger as="ul" className="flex flex-col gap-3">
          {members.map((row) => (
            <MemberRow
              key={`${row.user.id}-${row.currency}`}
              row={row}
              scale={scale}
              isMe={row.user.id === myId}
            />
          ))}
        </Stagger>
      </section>
      {pairs.length > 0 && (
        <section>
          <Heading>Kto komu ile</Heading>
          <ul className="flex flex-col gap-2">
            {pairs.map((p) => (
              <PairRow
                key={`${p.debtor.id}-${p.creditor.id}-${p.currency}`}
                pair={p}
                myId={myId}
                pending={pendingFrom(p)}
              />
            ))}
          </ul>
        </section>
      )}
    </>
  )
}

function ListSkeleton({ label }: { label: string }) {
  return (
    <div className="mt-6 flex flex-col gap-3" role="status" aria-label={label}>
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-20" />
      ))}
    </div>
  )
}

function LoadError({ label, onRetry }: { label: string; onRetry: () => void }) {
  return (
    <div className="mt-6 flex flex-col items-start gap-2">
      <span role="alert" className="text-base text-destructive">
        Nie udało się wczytać {label}.
      </span>
      <Button variant="outline" onClick={onRetry}>
        Spróbuj ponownie
      </Button>
    </div>
  )
}

/** Rozliczenia, as in Tricount: every entry in order, and a tab with the totals and who pays whom. */
export function LedgerPage() {
  const { data: me } = useMe()
  const [tab, setTab] = useState<Tab>('entries')

  return (
    <>
      <BackLink to="/pacts">Zakłady</BackLink>
      <h2 className="mb-4 text-2xl font-semibold">Rozliczenia</h2>
      <div className="flex gap-2" role="group" aria-label="Widok">
        {TABS.map(({ key, label }) => (
          <Button
            key={key}
            variant={tab === key ? 'default' : 'outline'}
            className="flex-1"
            aria-pressed={tab === key}
            onClick={() => setTab(key)}
          >
            {label}
          </Button>
        ))}
      </div>
      {tab === 'entries' ? <EntriesTab myId={me?.id} /> : <BalanceTab myId={me?.id} />}
    </>
  )
}
