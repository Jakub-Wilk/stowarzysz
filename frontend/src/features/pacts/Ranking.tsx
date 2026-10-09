import { Stagger } from '@/components/motion/Stagger'
import { Skeleton } from '@/components/ui/skeleton'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { usePactStats } from '@/features/pacts/hooks'
import { toneClasses } from '@/features/voting/tone'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

/** Wins, losses and money per member across settled pacts (confirmed results only). */
export function Ranking() {
  const stats = usePactStats()

  if (stats.isPending) {
    return (
      <div className="flex flex-col gap-3" role="status" aria-label="Ładowanie rankingu">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-20" />
        ))}
      </div>
    )
  }
  if (stats.isError) {
    return (
      <p role="alert" className="text-base text-destructive">
        Nie udało się wczytać rankingu.
      </p>
    )
  }
  if (stats.data.length === 0) {
    return (
      <p className="mt-6 text-center text-base text-muted-foreground">
        Nikt jeszcze nie rozstrzygnął żadnego zakładu.
      </p>
    )
  }
  return (
    <Stagger as="ul" className="flex flex-col gap-3">
      {stats.data.map((row, index) => {
        const net = row.money_won - row.money_lost
        return (
          <li key={row.user.id} className="flex items-center gap-4 rounded-lg border bg-card p-4">
            <span className="w-6 text-center text-xl font-black tabular-nums">{index + 1}</span>
            <UserAvatar username={row.user.username} src={row.user.avatar_url} size="md" />
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="text-metal truncate text-lg font-semibold">{row.user.username}</span>
              <span className="text-sm text-muted-foreground">
                <span className={toneClasses('positive').text}>{row.won} wygr.</span>
                {' · '}
                <span className={toneClasses('negative').text}>{row.lost} przegr.</span>
                {row.draw > 0 && ` · ${row.draw} remis.`}
              </span>
            </div>
            {(row.money_won > 0 || row.money_lost > 0) && (
              <span
                className={cn(
                  'text-base font-bold tabular-nums',
                  toneClasses(net > 0 ? 'positive' : net < 0 ? 'negative' : 'neutral').text,
                )}
                title={`Wygrane ${formatMoney(row.money_won)}, przegrane ${formatMoney(row.money_lost)}`}
              >
                {net > 0 ? '+' : ''}
                {formatMoney(net)}
              </span>
            )}
          </li>
        )
      })}
    </Stagger>
  )
}
