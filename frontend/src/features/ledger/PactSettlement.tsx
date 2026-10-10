import { ChevronRight } from 'lucide-react'
import { Link } from 'react-router'

import { who } from '@/features/ledger/format'
import { usePactEntries } from '@/features/ledger/hooks'
import { formatDebt } from '@/lib/money'

/** What a settled pact put in Kasa Skarbowa, linking there: the pact side of the integration. */
export function PactSettlement({ pactId, myId }: { pactId: number; myId: number | undefined }) {
  const entries = usePactEntries(pactId)
  const rows = (entries.data ?? []).filter((e) => e.status === 'confirmed')
  if (rows.length === 0) return null

  return (
    <section
      className="flex flex-col gap-2 rounded-lg border bg-card px-4 py-3"
      aria-label="Rozliczenie"
    >
      <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
        Rozliczenie
      </h3>
      {rows.map((entry) => (
        <Link
          key={entry.id}
          to={`/ledger/${entry.id}`}
          className="flex items-center gap-3 rounded-md py-1 hover:bg-accent"
        >
          <ul className="min-w-0 flex-1 text-base">
            {entry.obligations.map((o, i) => (
              <li key={i} className="flex justify-between gap-3">
                <span className="truncate">
                  {who(o.debtor, myId)} → {who(o.creditor, myId)}
                </span>
                <span className="font-semibold tabular-nums">{formatDebt(o)}</span>
              </li>
            ))}
          </ul>
          <ChevronRight className="size-5 shrink-0" aria-label="Zobacz w Kasie Skarbowej" />
        </Link>
      ))}
    </section>
  )
}
