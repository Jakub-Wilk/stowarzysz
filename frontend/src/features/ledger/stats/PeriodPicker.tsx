import { ChevronLeft, ChevronRight } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Segmented } from '@/features/ledger/form/controls'
import { todayInput } from '@/features/ledger/format'
import { currentMonth, isLatest, labelOf, type Period, shift } from '@/features/ledger/stats/period'

type Kind = Period['kind']

const KINDS: [Kind, string][] = [
  ['month', 'Miesiąc'],
  ['year', 'Rok'],
  ['all', 'Całość'],
  ['range', 'Zakres'],
]

function switchTo(kind: Kind, from: Period): Period {
  const now = new Date()
  switch (kind) {
    case 'month':
      return from.kind === 'month' ? from : currentMonth(now)
    case 'year':
      return { kind: 'year', year: from.kind === 'year' ? from.year : now.getFullYear() }
    case 'all':
      return { kind: 'all' }
    case 'range': {
      if (from.kind === 'range') return from
      const end = todayInput()
      const start = new Date(now.getFullYear(), now.getMonth(), 1)
      const pad = (n: number) => String(n).padStart(2, '0')
      return {
        kind: 'range',
        start: `${start.getFullYear()}-${pad(start.getMonth() + 1)}-01`,
        end,
      }
    }
  }
}

/** Pick what the stats cover: a month or year (with arrows to step through them), everything,
 * or any two dates. */
export function PeriodPicker({
  period,
  onChange,
}: {
  period: Period
  onChange: (period: Period) => void
}) {
  return (
    <div className="flex flex-col gap-3">
      <Segmented
        label="Okres"
        size="sm"
        options={KINDS}
        value={period.kind}
        onChange={(kind) => onChange(switchTo(kind, period))}
      />
      {(period.kind === 'month' || period.kind === 'year') && (
        <div className="flex items-center justify-between gap-2">
          <Button
            variant="outline"
            size="icon"
            aria-label={period.kind === 'month' ? 'Poprzedni miesiąc' : 'Poprzedni rok'}
            onClick={() => onChange(shift(period, -1))}
          >
            <ChevronLeft />
          </Button>
          <span className="text-metal text-lg font-semibold first-letter:uppercase">
            {labelOf(period)}
          </span>
          <Button
            variant="outline"
            size="icon"
            aria-label={period.kind === 'month' ? 'Następny miesiąc' : 'Następny rok'}
            disabled={isLatest(period)}
            onClick={() => onChange(shift(period, 1))}
          >
            <ChevronRight />
          </Button>
        </div>
      )}
      {period.kind === 'range' && (
        <div className="grid grid-cols-2 gap-2">
          <label className="flex flex-col gap-1 text-sm text-muted-foreground">
            Od
            <Input
              type="date"
              max={period.end}
              value={period.start}
              onChange={(e) => e.target.value && onChange({ ...period, start: e.target.value })}
            />
          </label>
          <label className="flex flex-col gap-1 text-sm text-muted-foreground">
            Do
            <Input
              type="date"
              min={period.start}
              max={todayInput()}
              value={period.end}
              onChange={(e) => e.target.value && onChange({ ...period, end: e.target.value })}
            />
          </label>
        </div>
      )}
    </div>
  )
}
