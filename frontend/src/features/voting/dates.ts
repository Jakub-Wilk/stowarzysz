import { DAY_FORMS, plural } from '@/lib/plural'

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()

/** Heading a history entry is filed under: today, yesterday, the weekday or the month. */
export function dayGroup(iso: string, now: Date = new Date()): string {
  const date = new Date(iso)
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000)
  if (days <= 0) return 'Dzisiaj'
  if (days === 1) return 'Wczoraj'
  if (days < 7) return date.toLocaleDateString('pl-PL', { weekday: 'long' })
  return date.toLocaleDateString('pl-PL', { month: 'long', year: 'numeric' })
}

export function formatWhen(iso: string): string {
  return new Date(iso).toLocaleString('pl-PL', {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/** Every vote ends this long after it was called (mirrors `POLL_DURATION` on the server). */
const POLL_DURATION_MS = 72 * 3_600_000

export const pollDeadline = (createdAt: string): string =>
  new Date(new Date(createdAt).getTime() + POLL_DURATION_MS).toISOString()

/** Time until the deadline as "2 dni 3 godz.", "5 godz. 10 min" or "12 min" (abbreviations don't inflect). */
export function timeLeft(createdAt: string, now: number = Date.now()): string {
  const minutes = Math.max(
    0,
    Math.ceil((new Date(pollDeadline(createdAt)).getTime() - now) / 60_000),
  )
  const days = Math.floor(minutes / 1440)
  const hours = Math.floor((minutes % 1440) / 60)
  const mins = minutes % 60
  if (days > 0) return `${days} ${plural(days, DAY_FORMS)}${hours ? ` ${hours} godz.` : ''}`
  if (hours > 0) return `${hours} godz.${mins ? ` ${mins} min` : ''}`
  return `${mins} min`
}
