import { UserAvatar } from '@/features/auth/UserAvatar'
import { who } from '@/features/ledger/format'
import type { LedgerStatsPerson } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

function Bar({ value, scale, className }: { value: number; scale: number; className: string }) {
  const width = scale > 0 ? (Math.max(value, 0) / scale) * 100 : 0
  return (
    <span className="block h-2 rounded-full bg-muted" aria-hidden>
      <span className={cn('block h-full rounded-full', className)} style={{ width: `${width}%` }} />
    </span>
  )
}

/** Each person's obligations (their part of the costs) and financial contribution (what they
 * paid out), with bars on one common scale. */
export function PeopleTotals({
  people,
  myId,
}: {
  people: LedgerStatsPerson[]
  myId: number | undefined
}) {
  const scale = Math.max(0, ...people.flatMap((p) => [p.share, p.paid]))
  return (
    <>
      <p className="mb-3 text-sm text-muted-foreground">
        Zobowiązania to część kosztów przypadająca na osobę, a wkład finansowy to to, co zapłaciła.
      </p>
      <ul className="flex flex-col gap-3">
        {people.map((p) => (
          <li
            key={p.user.id}
            className={cn(
              'flex flex-col gap-2 rounded-lg border bg-card p-4',
              p.user.id === myId && 'border-primary/50',
            )}
          >
            <div className="flex items-center gap-3">
              <UserAvatar username={p.user.username} src={p.user.avatar_url} size="md" />
              <span className="text-metal min-w-0 flex-1 truncate text-lg font-semibold">
                {who(p.user, myId)}
              </span>
            </div>
            <div className="grid grid-cols-[8rem_1fr_auto] items-center gap-x-3 gap-y-1.5 text-sm">
              <span className="text-muted-foreground">Zobowiązania</span>
              <Bar value={p.share} scale={scale} className="bg-primary" />
              <span className="font-semibold tabular-nums">{formatMoney(p.share)}</span>
              <span className="text-muted-foreground">Wkład finansowy</span>
              <Bar value={p.paid} scale={scale} className="bg-foreground/40" />
              <span className="font-semibold tabular-nums">{formatMoney(p.paid)}</span>
            </div>
          </li>
        ))}
      </ul>
    </>
  )
}
