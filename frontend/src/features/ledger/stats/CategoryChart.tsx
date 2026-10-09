import { Cell, Pie, PieChart } from 'recharts'

import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '@/components/ui/chart'
import { categoryColor } from '@/features/ledger/stats/colors'
import type { LedgerMeta, LedgerStats } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { ENTRY_FORMS, plural } from '@/lib/plural'

/** Where the money went: a donut with the total in the middle, and every category listed with
 * its share. A category that was refunded more than it cost is listed but not drawn. */
export function CategoryChart({
  stats,
  meta,
}: {
  stats: LedgerStats
  meta: LedgerMeta | undefined
}) {
  const drawn = stats.categories.filter((c) => c.spent > 0)
  const config: ChartConfig = Object.fromEntries(
    stats.categories.map((c) => [c.key, { label: c.label }]),
  )
  const total = drawn.reduce((sum, c) => sum + c.spent, 0)

  return (
    <div className="flex flex-col gap-4">
      {drawn.length > 0 && (
        <div className="relative mx-auto w-full max-w-64">
          <ChartContainer config={config} className="aspect-square w-full">
            <PieChart>
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    hideLabel
                    nameKey="key"
                    formatter={(value, _name, item) => (
                      <span className="flex w-full justify-between gap-3">
                        <span>{item.payload.label}</span>
                        <span className="font-semibold tabular-nums">
                          {formatMoney(Number(value))}
                        </span>
                      </span>
                    )}
                  />
                }
              />
              <Pie
                data={drawn}
                dataKey="spent"
                nameKey="key"
                innerRadius="62%"
                outerRadius="95%"
                paddingAngle={drawn.length > 1 ? 2 : 0}
                strokeWidth={0}
              >
                {drawn.map((c) => (
                  <Cell key={c.key} fill={categoryColor(c.key, meta)} />
                ))}
              </Pie>
            </PieChart>
          </ChartContainer>
          <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
            <span className="text-xs text-muted-foreground">Razem</span>
            <span className="text-xl font-bold tabular-nums">{formatMoney(stats.spent)}</span>
          </div>
        </div>
      )}
      <ul className="flex flex-col divide-y rounded-lg border bg-card px-3">
        {stats.categories.map((c) => {
          const percent = total > 0 && c.spent > 0 ? Math.round((c.spent / total) * 100) : null
          return (
            <li key={c.key} className="flex items-center gap-3 py-2.5">
              <span
                aria-hidden
                className="size-3 shrink-0 rounded-full"
                style={{ background: categoryColor(c.key, meta) }}
              />
              <span aria-hidden className="text-xl leading-none">
                {c.emoji}
              </span>
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-base">{c.label}</span>
                <span className="text-xs text-muted-foreground">
                  {c.count} {plural(c.count, ENTRY_FORMS)}
                  {percent !== null && ` · ${percent}%`}
                </span>
              </span>
              <span className="text-base font-semibold tabular-nums">{formatMoney(c.spent)}</span>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
