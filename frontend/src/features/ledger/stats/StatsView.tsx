import { useState } from 'react'

import { useCountUp } from '@/components/motion/useCountUp'
import { useLedgerMeta, useStats } from '@/features/ledger/hooks'
import { ListSkeleton, LoadError } from '@/features/ledger/LoadStates'
import { CategoryChart } from '@/features/ledger/stats/CategoryChart'
import { PeopleTotals } from '@/features/ledger/stats/PeopleTotals'
import { currentMonth, type Period, rangeOf } from '@/features/ledger/stats/period'
import { PeriodPicker } from '@/features/ledger/stats/PeriodPicker'
import { SpendingChart } from '@/features/ledger/stats/SpendingChart'
import { ENTRY_FORMS, plural } from '@/lib/plural'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

function Heading({ children }: { children: string }) {
  return <h3 className="mt-8 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

function Stat({
  label,
  value,
  note,
  big,
  wide,
}: {
  label: string
  value: number
  note?: string
  /** Large figure, full width. */
  big?: boolean
  /** Full width, normal size. */
  wide?: boolean
}) {
  const shown = useCountUp(value)
  return (
    <div
      className={cn(
        'flex flex-col gap-0.5 rounded-lg border bg-card p-4',
        (big || wide) && 'col-span-2',
      )}
    >
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className={cn('font-bold tabular-nums', big ? 'text-4xl' : 'text-xl')}>
        {formatMoney(shown)}
      </span>
      {note && <span className="text-sm text-muted-foreground">{note}</span>}
    </div>
  )
}

/** Statystyki: what the group spent in a month, a year, ever, or between two dates, by category
 * and by person, with charts. Expenses and money debts count; incomes reduce the total. */
export default function StatsView({ myId }: { myId: number | undefined }) {
  const [period, setPeriod] = useState<Period>(() => currentMonth())
  const { start, end } = rangeOf(period)
  const stats = useStats(start, end)
  const meta = useLedgerMeta()
  const mine = stats.data?.people.find((p) => p.user.id === myId)

  return (
    <div className="mt-6 flex flex-col">
      <PeriodPicker period={period} onChange={setPeriod} />
      {stats.isPending && <ListSkeleton label="Ładowanie statystyk" />}
      {stats.isError && <LoadError what="statystyk" onRetry={() => void stats.refetch()} />}
      {stats.data &&
        (stats.data.count === 0 ? (
          <p className="mt-10 text-center text-base text-muted-foreground">
            Brak wpisów w tym okresie.
          </p>
        ) : (
          <>
            <div className="mt-6 grid grid-cols-2 gap-3">
              <Stat
                big
                label="Wydano razem"
                value={stats.data.spent}
                note={`${stats.data.count} ${plural(stats.data.count, ENTRY_FORMS)}${
                  stats.data.monthly_average !== null
                    ? ` · średnio ${formatMoney(stats.data.monthly_average)} miesięcznie`
                    : ''
                }`}
              />
              <Stat label="Twoje zobowiązania" value={mine?.share ?? 0} />
              <Stat label="Twój wkład finansowy" value={mine?.paid ?? 0} />
              {stats.data.income > 0 && (
                <Stat
                  wide
                  label="Przychody"
                  value={stats.data.income}
                  note={`wydatki ${formatMoney(stats.data.expenses)}`}
                />
              )}
            </div>

            <Heading>{stats.data.unit === 'day' ? 'Dzień po dniu' : 'Miesiąc po miesiącu'}</Heading>
            <SpendingChart stats={stats.data} />

            <Heading>Kategorie</Heading>
            <CategoryChart stats={stats.data} meta={meta.data} />

            <Heading>Osoby</Heading>
            <PeopleTotals people={stats.data.people} myId={myId} />
          </>
        ))}
    </div>
  )
}
