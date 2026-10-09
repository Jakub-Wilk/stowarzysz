import { getPactKind } from '@/features/pacts/kinds'
import { toneClasses } from '@/features/voting/tone'
import type { PactKindKey, PactStats } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

function Stat({ label, value, className }: { label: string; value: string; className?: string }) {
  return (
    <div className="flex flex-col rounded-lg border bg-card px-3 py-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className={cn('text-lg font-semibold tabular-nums', className)}>{value}</span>
    </div>
  )
}

/** A member's pact record for the management profile: settled pacts only, confirmed results. */
export function PactStatsBlock({ stats }: { stats: PactStats }) {
  const net = stats.money_won - stats.money_lost
  const empty =
    stats.won + stats.lost + stats.draw === 0 && stats.money_won + stats.money_lost === 0
  const kinds = Object.entries(stats.by_kind) as [PactKindKey, PactStats['by_kind'][PactKindKey]][]

  return (
    <section className="mt-10 flex max-w-md flex-col gap-3 border-t pt-6">
      <h3 className="font-semibold">Zakłady</h3>
      {empty ? (
        <p className="text-sm text-muted-foreground">
          Brak rozstrzygniętych zakładów z udziałem tej osoby.
        </p>
      ) : (
        <>
          <div className="grid grid-cols-3 gap-2">
            <Stat
              label="Wygrane"
              value={String(stats.won)}
              className={toneClasses('positive').text}
            />
            <Stat
              label="Przegrane"
              value={String(stats.lost)}
              className={toneClasses('negative').text}
            />
            <Stat label="Remisy" value={String(stats.draw)} />
          </div>
          <div className="grid grid-cols-3 gap-2">
            <Stat label="Wygrana kwota" value={formatMoney(stats.money_won)} />
            <Stat label="Przegrana kwota" value={formatMoney(stats.money_lost)} />
            <Stat
              label="Saldo"
              value={`${net > 0 ? '+' : ''}${formatMoney(net)}`}
              className={toneClasses(net > 0 ? 'positive' : net < 0 ? 'negative' : 'neutral').text}
            />
          </div>
          {kinds.length > 0 && (
            <ul className="flex flex-col gap-1 text-sm text-muted-foreground">
              {kinds.map(([kind, counts]) => (
                <li key={kind}>
                  {getPactKind(kind).label}: {counts?.won ?? 0} wygr. · {counts?.lost ?? 0} przegr.
                  {counts && counts.draw > 0 ? ` · ${counts.draw} remis.` : ''}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  )
}
