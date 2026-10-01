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
