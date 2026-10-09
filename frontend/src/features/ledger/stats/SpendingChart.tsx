import { Bar, BarChart, CartesianGrid, XAxis } from 'recharts'

import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '@/components/ui/chart'
import { pointLabel, pointTitle } from '@/features/ledger/stats/period'
import type { LedgerStats } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'

const config = { spent: { label: 'Wydano', color: 'var(--primary)' } } satisfies ChartConfig

/** What was spent over the period, a bar per day (up to about two months) or per month. */
export function SpendingChart({ stats }: { stats: LedgerStats }) {
  return (
    <ChartContainer config={config} className="aspect-[16/10] w-full">
      <BarChart data={stats.series} margin={{ left: 0, right: 0, top: 8 }}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="period"
          tickLine={false}
          axisLine={false}
          tickMargin={8}
          interval={stats.unit === 'month' ? 0 : 'preserveStartEnd'}
          minTickGap={12}
          tickFormatter={(period: string) => pointLabel(period, stats.unit)}
        />
        <ChartTooltip
          cursor={false}
          content={
            <ChartTooltipContent
              hideIndicator
              labelFormatter={(_, items) =>
                pointTitle(String(items[0]?.payload?.period ?? ''), stats.unit)
              }
              formatter={(value) => (
                <span className="font-semibold tabular-nums">{formatMoney(Number(value))}</span>
              )}
            />
          }
        />
        <Bar dataKey="spent" fill="var(--color-spent)" radius={4} />
      </BarChart>
    </ChartContainer>
  )
}
