import { Link } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { useBalances, useEntries, useEntryAction } from '@/features/ledger/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import { formatDate } from '@/features/secretsanta/format'
import { toneClasses } from '@/features/voting/tone'
import type { LedgerBalance, LedgerEntry } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

function Heading({ children }: { children: string }) {
  return <h3 className="mt-8 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

function Balance({ row }: { row: LedgerBalance }) {
  const owedToMe = row.amount > 0
  return (
    <li className="flex items-center gap-4 rounded-lg border bg-card p-4">
      <UserAvatar username={row.user.username} src={row.user.avatar_url} size="md" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-metal truncate text-lg font-semibold">{row.user.username}</span>
        <span className="text-sm text-muted-foreground">
          {owedToMe ? 'jest Ci winien(-na)' : 'jesteś winien(-na)'}
        </span>
      </div>
      <span
        className={cn(
          'text-xl font-bold tabular-nums',
          toneClasses(owedToMe ? 'positive' : 'negative').text,
        )}
      >
        {formatMoney(Math.abs(row.amount))}
      </span>
    </li>
  )
}

function Entry({ entry, myId }: { entry: LedgerEntry; myId: number | undefined }) {
  const action = useEntryAction()
  const iOwe = entry.debtor.id === myId
  const other = iOwe ? entry.creditor : entry.debtor
  const closed = entry.settled_at !== null
  const source =
    entry.source_type === 'pact' && entry.source_id !== null ? `/pacts/${entry.source_id}` : null

  return (
    <li className={cn('flex flex-col gap-3 rounded-lg border bg-card p-4', closed && 'opacity-60')}>
      <div className="flex items-center gap-4">
        <UserAvatar username={other.username} src={other.avatar_url} size="md" />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-metal truncate text-lg font-semibold">
            {iOwe ? `Płacisz: ${other.username}` : `${other.username} płaci Tobie`}
          </span>
          <span className="truncate text-sm text-muted-foreground">
            {source ? (
              <Link to={source} className="underline">
                {entry.description}
              </Link>
            ) : (
              entry.description
            )}
            {' · '}
            {formatDate(entry.created_at)}
          </span>
        </div>
        <span
          className={cn(
            'text-xl font-bold tabular-nums',
            toneClasses(iOwe ? 'negative' : 'positive').text,
          )}
        >
          {formatMoney(entry.amount)}
        </span>
      </div>
      {closed ? (
        <span className="text-sm text-muted-foreground">
          Rozliczone {formatDate(entry.settled_at as string)}
        </span>
      ) : iOwe ? (
        entry.paid_marked_at ? (
          <span className="text-sm text-muted-foreground">
            Oznaczone jako zapłacone, czeka na potwierdzenie przez {other.username}.
          </span>
        ) : (
          <Button
            variant="outline"
            disabled={action.isPending}
            onClick={() => action.mutate({ id: entry.id, action: 'paid' })}
          >
            Zapłaciłem(-am)
          </Button>
        )
      ) : (
        <Button
          variant={entry.paid_marked_at ? 'default' : 'outline'}
          disabled={action.isPending}
          onClick={() => action.mutate({ id: entry.id, action: 'confirm' })}
        >
          {entry.paid_marked_at
            ? `${other.username} zapłacił(a): potwierdzam`
            : 'Otrzymałem(-am) pieniądze'}
        </Button>
      )}
      <MutationError error={action.error} />
    </li>
  )
}

/** Who owes whom, net per person, and every debt with its mark-paid / confirm steps. */
export function LedgerPage() {
  const { data: me } = useMe()
  const balances = useBalances()
  const entries = useEntries()
  const open = entries.data?.filter((e) => e.settled_at === null) ?? []
  const history = entries.data?.filter((e) => e.settled_at !== null) ?? []

  return (
    <>
      <BackLink to="/pacts">Zakłady</BackLink>
      <h2 className="mb-2 text-2xl font-semibold">Rozliczenia</h2>

      {(balances.isError || entries.isError) && (
        <div className="mt-4 flex flex-col items-start gap-2">
          <span role="alert" className="text-base text-destructive">
            Nie udało się wczytać rozliczeń.
          </span>
          <Button
            variant="outline"
            onClick={() => {
              void balances.refetch()
              void entries.refetch()
            }}
          >
            Spróbuj ponownie
          </Button>
        </div>
      )}

      {(balances.isPending || entries.isPending) && (
        <div className="mt-6 flex flex-col gap-3" role="status" aria-label="Ładowanie rozliczeń">
          {[0, 1].map((i) => (
            <Skeleton key={i} className="h-20" />
          ))}
        </div>
      )}

      {entries.isSuccess && entries.data.length === 0 && (
        <p className="mt-10 text-center text-base text-muted-foreground">
          Nikt nikomu nic nie jest winien. Na razie.
        </p>
      )}

      {balances.data && balances.data.length > 0 && (
        <section>
          <Heading>Saldo</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {balances.data.map((row) => (
              <Balance key={`${row.user.id}-${row.currency}`} row={row} />
            ))}
          </Stagger>
        </section>
      )}

      {open.length > 0 && (
        <section>
          <Heading>Do rozliczenia</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {open.map((entry) => (
              <Entry key={entry.id} entry={entry} myId={me?.id} />
            ))}
          </Stagger>
        </section>
      )}

      {history.length > 0 && (
        <section>
          <Heading>Historia</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {history.map((entry) => (
              <Entry key={entry.id} entry={entry} myId={me?.id} />
            ))}
          </Stagger>
        </section>
      )}
    </>
  )
}
