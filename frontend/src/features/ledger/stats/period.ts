/** The periods the stats view can show, and the dates they mean (local `YYYY-MM-DD`). */
export type Period =
  | { kind: 'month'; year: number; month: number } // month: 0-11
  | { kind: 'year'; year: number }
  | { kind: 'all' }
  | { kind: 'range'; start: string; end: string }

const pad = (n: number) => String(n).padStart(2, '0')
const iso = (year: number, month: number, day: number) => `${year}-${pad(month + 1)}-${pad(day)}`
const lastDay = (year: number, month: number) => new Date(year, month + 1, 0).getDate()

export function currentMonth(now: Date = new Date()): Period {
  return { kind: 'month', year: now.getFullYear(), month: now.getMonth() }
}

/** The dates a period covers; `null` for both means everything. */
export function rangeOf(period: Period): { start: string | null; end: string | null } {
  switch (period.kind) {
    case 'month':
      return {
        start: iso(period.year, period.month, 1),
        end: iso(period.year, period.month, lastDay(period.year, period.month)),
      }
    case 'year':
      return { start: iso(period.year, 0, 1), end: iso(period.year, 11, 31) }
    case 'all':
      return { start: null, end: null }
    case 'range':
      return { start: period.start, end: period.end }
  }
}

/** The previous (-1) or next (+1) month or year; other periods don't move. */
export function shift(period: Period, by: 1 | -1): Period {
  if (period.kind === 'year') return { kind: 'year', year: period.year + by }
  if (period.kind !== 'month') return period
  const index = period.year * 12 + period.month + by
  return { kind: 'month', year: Math.floor(index / 12), month: index % 12 }
}

/** Whether stepping forward would leave the present (there is nothing to show there). */
export function isLatest(period: Period, now: Date = new Date()): boolean {
  if (period.kind === 'month') {
    return period.year * 12 + period.month >= now.getFullYear() * 12 + now.getMonth()
  }
  return period.kind === 'year' ? period.year >= now.getFullYear() : true
}

const monthName = (year: number, month: number) =>
  new Date(year, month, 1).toLocaleDateString('pl-PL', { month: 'long', year: 'numeric' })

export function labelOf(period: Period): string {
  switch (period.kind) {
    case 'month':
      return monthName(period.year, period.month)
    case 'year':
      return String(period.year)
    case 'all':
      return 'Od początku'
    case 'range':
      return `${shortDate(period.start)} – ${shortDate(period.end)}`
  }
}

const shortDate = (day: string): string =>
  new Date(`${day}T00:00`).toLocaleDateString('pl-PL', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  })

/** A chart axis label for a series point: the day of the month, or the short month name. */
export function pointLabel(period: string, unit: 'day' | 'month'): string {
  if (unit === 'day') return String(Number(period.slice(8)))
  return new Date(`${period}-01T00:00`).toLocaleDateString('pl-PL', { month: 'short' })
}

/** A tooltip title for a series point: the full date, or the month and year. */
export function pointTitle(period: string, unit: 'day' | 'month'): string {
  const date = new Date(unit === 'day' ? `${period}T00:00` : `${period}-01T00:00`)
  return unit === 'day'
    ? date.toLocaleDateString('pl-PL', { day: 'numeric', month: 'long', year: 'numeric' })
    : date.toLocaleDateString('pl-PL', { month: 'long', year: 'numeric' })
}
