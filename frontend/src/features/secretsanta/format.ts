export const formatAmount = (amount: number) => `${amount} zł`

export const formatAmounts = (amounts: number[]) => amounts.map(formatAmount).join(', ')

export const formatDate = (iso: string) =>
  new Date(iso).toLocaleDateString('pl-PL', { day: 'numeric', month: 'long', year: 'numeric' })

export const formatDateTime = (iso: string) =>
  new Date(iso).toLocaleString('pl-PL', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })

const pad = (n: number) => String(n).padStart(2, '0')

/** ISO instant -> the local `YYYY-MM-DDTHH:mm` an `<input type="datetime-local">` wants. */
export function toLocalInput(iso: string): string {
  const d = new Date(iso)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** Whole days from now until `iso` (negative once passed), rounded up. */
export function daysUntil(iso: string, now: Date = new Date()): number {
  return Math.ceil((new Date(iso).getTime() - now.getTime()) / 86_400_000)
}

/** Form text -> positive whole amounts, or null if any entry is blank or invalid. */
export function parseTiers(values: string[]): number[] | null {
  const parsed = values.map((v) => Number(v))
  return parsed.every((n) => Number.isInteger(n) && n > 0) ? parsed : null
}
