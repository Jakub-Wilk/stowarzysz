import { formatScore } from '@/features/voting/format'
import type { VotingStats } from '@/lib/api-types'
import { plural, VOTE_FORMS } from '@/lib/plural'

const VETO_FORMS = { one: 'weto', few: 'weta', many: 'wet' }

/** One compact line for list rows. Counts finished votes only (open ones stay hidden). */
export function VotingStatsLine({ stats }: { stats: VotingStats }) {
  if (stats.votes_cast === 0) {
    return <span className="text-sm text-muted-foreground">Brak głosów</span>
  }
  return (
    <span className="text-sm text-muted-foreground">
      śr. {stats.average_score === null ? '—' : formatScore(stats.average_score)} ·{' '}
      {stats.veto_count} {plural(stats.veto_count, VETO_FORMS)} ({stats.veto_percent}%) ·{' '}
      {stats.votes_cast} {plural(stats.votes_cast, VOTE_FORMS)}
    </span>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col rounded-lg border bg-card px-3 py-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="text-lg font-semibold tabular-nums">{value}</span>
    </div>
  )
}

/** Fuller breakdown for the edit page. */
export function VotingStatsBlock({ stats }: { stats: VotingStats }) {
  return (
    <section className="mt-10 flex max-w-md flex-col gap-3 border-t pt-6">
      <h3 className="font-semibold">Głosowania</h3>
      {stats.votes_cast === 0 ? (
        <p className="text-sm text-muted-foreground">
          Brak zakończonych głosowań z udziałem tej osoby.
        </p>
      ) : (
        <div className="grid grid-cols-2 gap-2">
          <Stat label="Oddane głosy" value={String(stats.votes_cast)} />
          <Stat
            label="Średni wynik"
            value={stats.average_score === null ? '—' : formatScore(stats.average_score)}
          />
          <Stat label="Weta" value={String(stats.veto_count)} />
          <Stat label="Odsetek wet" value={`${stats.veto_percent}%`} />
        </div>
      )}
    </section>
  )
}
