import { HandCoins, Plus } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { useMe } from '@/features/auth/hooks'
import { BalanceView } from '@/features/ledger/BalanceView'
import { EntryRow } from '@/features/ledger/EntryRow'
import { GoodsDebtDialog } from '@/features/ledger/GoodsDebtDialog'
import { useFeed } from '@/features/ledger/hooks'
import { ListSkeleton, LoadError } from '@/features/ledger/LoadStates'
import { dayGroup } from '@/features/voting/dates'
import type { LedgerEntry } from '@/lib/api-types'

type Segment = 'entries' | 'balance'

const SEGMENTS: { key: Segment; label: string }[] = [
  { key: 'entries', label: 'Wpisy' },
  { key: 'balance', label: 'Bilans' },
]

function Heading({ children }: { children: string }) {
  return <h3 className="mt-6 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

function groupByDay(entries: LedgerEntry[]): [string, LedgerEntry[]][] {
  const groups = new Map<string, LedgerEntry[]>()
  for (const entry of entries) {
    const label = dayGroup(`${entry.occurred_on}T00:00`)
    groups.set(label, [...(groups.get(label) ?? []), entry])
  }
  return [...groups]
}

/** Everything that happened, newest first and filed under its day, like Tricount's list. */
function Feed({ myId }: { myId: number | undefined }) {
  const feed = useFeed()
  const sentinel = useRef<HTMLDivElement>(null)
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = feed

  // load the next page when the end of the list scrolls into view
  useEffect(() => {
    const node = sentinel.current
    if (!node || !hasNextPage) return
    const observer = new IntersectionObserver((seen) => {
      if (seen.some((e) => e.isIntersecting) && !isFetchingNextPage) void fetchNextPage()
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [hasNextPage, isFetchingNextPage, fetchNextPage])

  if (feed.isPending) return <ListSkeleton label="Ładowanie wpisów" />
  if (feed.isError) return <LoadError what="wpisów" onRetry={() => void feed.refetch()} />
  const entries = feed.data.pages.flatMap((page) => page.results)
  if (entries.length === 0) {
    return (
      <p className="mt-10 text-center text-base text-muted-foreground">
        Nic tu jeszcze nie ma. Dodaj pierwszy wspólny wydatek.
      </p>
    )
  }
  return (
    <>
      {groupByDay(entries).map(([label, rows]) => (
        <section key={label}>
          <Heading>{label}</Heading>
          <Stagger as="ul" className="flex flex-col gap-3">
            {rows.map((entry) => (
              <EntryRow key={entry.id} entry={entry} myId={myId} />
            ))}
          </Stagger>
        </section>
      ))}
      <div ref={sentinel} />
      {hasNextPage && (
        <div className="mt-4 flex justify-center">
          <Button variant="ghost" disabled={isFetchingNextPage} onClick={() => fetchNextPage()}>
            {isFetchingNextPage ? 'Ładowanie…' : 'Pokaż starsze'}
          </Button>
        </div>
      )}
    </>
  )
}

/** Rozliczenia, as in Tricount: shared expenses, debts from pacts and paybacks, plus the Bilans
 * with who should pay whom. */
export function LedgerPage() {
  const { data: me } = useMe()
  const [segment, setSegment] = useState<Segment>('entries')
  const [owedGoods, setOwedGoods] = useState(false)

  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-2xl font-semibold">Rozliczenia</h2>
        <div className="flex items-center gap-2">
          {me && (
            <Button
              variant="outline"
              size="icon"
              aria-label="Ktoś jest mi winien coś innego niż pieniądze"
              title="Dług w naturze"
              onClick={() => setOwedGoods(true)}
            >
              <HandCoins />
            </Button>
          )}
          <Button nativeButton={false} render={<Link to="/ledger/new" />}>
            <Plus /> Wydatek
          </Button>
        </div>
      </div>
      {me && <GoodsDebtDialog myId={me.id} open={owedGoods} onOpenChange={setOwedGoods} />}

      <div className="mt-6 flex gap-2" role="group" aria-label="Widok">
        {SEGMENTS.map(({ key, label }) => (
          <Button
            key={key}
            variant={segment === key ? 'default' : 'outline'}
            className="flex-1"
            aria-pressed={segment === key}
            onClick={() => setSegment(key)}
          >
            {label}
          </Button>
        ))}
      </div>

      {segment === 'entries' ? <Feed myId={me?.id} /> : <BalanceView myId={me?.id} />}
    </>
  )
}
