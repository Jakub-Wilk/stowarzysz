import { Link } from 'react-router'

import { UserAvatar } from '@/features/auth/UserAvatar'
import { who } from '@/features/ledger/format'
import type { DebtEntry } from '@/lib/api-types'
import { formatDebt } from '@/lib/money'

export function DebtDetail({ entry, myId }: { entry: DebtEntry; myId: number | undefined }) {
  return (
    <section className="flex flex-col gap-1 rounded-lg border bg-card px-4 py-3">
      <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
        Kto komu
      </h3>
      <ul className="divide-y">
        {entry.obligations.map((o, i) => (
          <li key={i} className="flex items-center gap-3 py-2">
            <UserAvatar username={o.debtor.username} src={o.debtor.avatar_url} size="sm" />
            <span className="min-w-0 flex-1 text-base">
              <strong>{who(o.debtor, myId)}</strong> → <strong>{who(o.creditor, myId)}</strong>
            </span>
            <span className="text-base font-semibold tabular-nums">{formatDebt(o)}</span>
          </li>
        ))}
      </ul>
      {entry.source_type === 'pact' && entry.source_id !== null && (
        <Link to={`/pacts/${entry.source_id}`} className="mt-2 self-start text-base underline">
          Zobacz zakład
        </Link>
      )}
    </section>
  )
}
