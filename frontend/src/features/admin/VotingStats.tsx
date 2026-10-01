import { formatScore } from '@/features/voting/format'
import type { VotingStats } from '@/lib/api-types'

const plural = (n: number, singular: string, pluralForm = `${singular}s`) =>
  `${n} ${n === 1 ? singular : pluralForm}`

/** One compact line for list rows. Counts finished votes only (open ones stay hidden). */
export function VotingStatsLine({ stats }: { stats: VotingStats }) {
  if (stats.votes_cast === 0) {
    return <span className="text-sm text-muted-foreground">No votes yet</span>
  }
  return (
    <span className="text-sm text-muted-foreground">
      avg {stats.average_score === null ? '—' : formatScore(stats.average_score)} ·{' '}
      {plural(stats.veto_count, 'veto', 'vetoes')} ({stats.veto_percent}%) ·{' '}
      {plural(stats.votes_cast, 'vote')}
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
      <h3 className="font-semibold">Voting</h3>
      {stats.votes_cast === 0 ? (
        <p className="text-sm text-muted-foreground">Hasn&apos;t voted in a finished vote yet.</p>
      ) : (
        <div className="grid grid-cols-2 gap-2">
          <Stat label="Votes cast" value={String(stats.votes_cast)} />
          <Stat
            label="Average score"
            value={stats.average_score === null ? '—' : formatScore(stats.average_score)}
          />
          <Stat label="Vetoes" value={String(stats.veto_count)} />
          <Stat label="Veto rate" value={`${stats.veto_percent}%`} />
        </div>
      )}
    </section>
  )
}
