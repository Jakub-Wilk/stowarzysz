import { useLedgerMeta } from '@/features/ledger/hooks'
import type { LedgerCategoryInfo, LedgerEntry, Tone, UserBrief } from '@/lib/api-types'
import { formatDebt, formatMoney } from '@/lib/money'

/** "Ty" for the signed-in user, otherwise the username. */
export const who = (user: UserBrief, myId: number | undefined): string =>
  user.id === myId ? 'Ty' : user.username

/** `YYYY-MM-DD` as a local date (not UTC midnight, which can be the day before). */
export const localDate = (day: string): Date => new Date(`${day}T00:00`)

export const formatDay = (day: string): string =>
  localDate(day).toLocaleDateString('pl-PL', { day: 'numeric', month: 'long', year: 'numeric' })

/** Today as `YYYY-MM-DD`, the way `<input type="date">` wants it. */
export function todayInput(): string {
  const now = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

/** Formatting that knows every currency's minor-unit digits (from the server; 2 until loaded). */
export function useMoney() {
  const meta = useLedgerMeta()
  const exponent = (currency: string): number =>
    meta.data?.currencies.find((c) => c.code === currency)?.exponent ?? 2
  return {
    exponent,
    /** An entry's headline: money in its currency, goods, or nothing for a mixed entry. */
    headline: (entry: LedgerEntry): string | null => {
      if (entry.amount === null) return null
      if (entry.item) return formatDebt({ amount: entry.amount, item: entry.item })
      return formatMoney(entry.amount, entry.currency, exponent(entry.currency))
    },
    format: (minor: number, currency: string): string =>
      formatMoney(minor, currency, exponent(currency)),
  }
}

/** An entry's category (name and emoji), or nothing for an uncategorized entry. */
export function useCategory(): (entry: LedgerEntry) => LedgerCategoryInfo | undefined {
  const meta = useLedgerMeta()
  return (entry) => meta.data?.categories.find((c) => c.key === entry.category)
}

/** What an entry means for one person: money (grosze) and goods they gain (+) or owe (-). */
export interface Impact {
  money: number
  goods: { item: string; amount: number }[]
}

export function impactOf(entry: LedgerEntry, userId: number | undefined): Impact | null {
  if (userId === undefined) return null
  let money = 0
  const goods = new Map<string, { item: string; amount: number }>()
  let involved = false
  for (const o of entry.obligations) {
    const sign = o.creditor.id === userId ? 1 : o.debtor.id === userId ? -1 : 0
    if (!sign) continue
    involved = true
    if (!o.item) {
      money += sign * o.amount
      continue
    }
    const key = o.item.toLowerCase()
    const current = goods.get(key) ?? { item: o.item, amount: 0 }
    goods.set(key, { ...current, amount: current.amount + sign * o.amount })
  }
  if (!involved) return null
  return { money, goods: [...goods.values()].filter((g) => g.amount !== 0) }
}

/** "+60 zł", "-30 zł, -1 × piwo"; null when it evens out. */
export function formatImpact(impact: Impact): string | null {
  const parts: string[] = []
  if (impact.money) parts.push(`${impact.money > 0 ? '+' : ''}${formatMoney(impact.money)}`)
  for (const g of impact.goods) {
    const sign = g.amount > 0 ? '+' : '-'
    parts.push(`${sign}${formatDebt({ amount: Math.abs(g.amount), item: g.item })}`)
  }
  return parts.length ? parts.join(', ') : null
}

export const impactTone = (impact: Impact): Tone =>
  impact.money > 0 || impact.goods.some((g) => g.amount > 0)
    ? 'positive'
    : impact.money < 0 || impact.goods.some((g) => g.amount < 0)
      ? 'negative'
      : 'neutral'

/** "3" -> 3; null for anything that is not a whole number from 1 to 1000. */
export function parseQuantity(text: string): number | null {
  const n = Number(text.trim())
  return Number.isInteger(n) && n >= 1 && n <= 1000 ? n : null
}
