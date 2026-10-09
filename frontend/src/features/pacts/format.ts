import type { PactParticipant, PactParticipantState, PactStatus, Tone } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { DAY_FORMS, plural } from '@/lib/plural'

export const STATUS_LABEL: Record<PactStatus, string> = {
  proposed: 'Czeka na odpowiedzi',
  active: 'W toku',
  awaiting_result: 'Czeka na wynik',
  resolved: 'Rozstrzygnięty',
  declined: 'Odrzucony',
  cancelled: 'Anulowany',
  void: 'Unieważniony',
}

export const STATUS_TONE: Record<PactStatus, Tone> = {
  proposed: 'neutral',
  active: 'neutral',
  awaiting_result: 'neutral',
  resolved: 'positive',
  declined: 'negative',
  cancelled: 'negative',
  void: 'negative',
}

/** Pacts that are over (their result, if any, is final). */
export const isArchived = (status: PactStatus) =>
  status === 'resolved' || status === 'declined' || status === 'cancelled' || status === 'void'

export const PARTICIPANT_STATE_LABEL: Record<PactParticipantState, string> = {
  invited: 'zaproszony(-a)',
  requested: 'prosi o dołączenie',
  active: 'bierze udział',
  settled: 'rozliczony(-a)',
  void: 'unieważniony(-a)',
  declined: 'odmówił(a)',
  rejected: 'odrzucony(-a)',
  withdrawn: 'wycofał(a) się',
  expired: 'zaproszenie wygasło',
}

/** "10 zł", "kolacja" or "10 zł + kolacja"; null when nothing was staked. */
export function formatStake(
  p: Pick<PactParticipant, 'stake_amount' | 'stake_note'>,
): string | null {
  const parts = [p.stake_amount ? formatMoney(p.stake_amount) : null, p.stake_note || null]
  const text = parts.filter(Boolean).join(' + ')
  return text || null
}

/** "za 3 dni", "dzisiaj" or "3 dni temu" relative to `now`. */
export function dueLabel(iso: string, now: Date = new Date()): string {
  const days = Math.ceil((new Date(iso).getTime() - now.getTime()) / 86_400_000)
  if (days === 0) return 'dzisiaj'
  if (days > 0) return `za ${days} ${plural(days, DAY_FORMS)}`
  return `${-days} ${plural(-days, DAY_FORMS)} temu`
}

/** A `<input type="date">` value -> the end of that local day as an ISO instant. */
export function endOfDayIso(date: string): string {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(y, m - 1, d, 23, 59).toISOString()
}

/** Tomorrow as a `<input type="date">` value (deadlines must be in the future). */
export function tomorrowInput(now: Date = new Date()): string {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}
