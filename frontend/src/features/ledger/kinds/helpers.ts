import type { ExpenseEntry, PaymentEntry, UserBrief } from '@/lib/api-types'

/** A plain expense is one item named after the title (split any way): nothing to list. A
 * receipt has lines. */
export function isItemized(entry: ExpenseEntry): boolean {
  const items = entry.details.items
  return items.length > 1 || items[0]?.name !== entry.title
}

/** Everyone an expense mentions, by id (its breakdown lists them all). */
export const peopleOf = (entry: ExpenseEntry): Map<number, UserBrief> =>
  new Map(entry.breakdown.map((row) => [row.user.id, row.user]))

/** A payment's obligation runs from the receiver back to the payer (that is what pays the debt
 * off), so the parties read the other way round. */
export function partiesOf(entry: PaymentEntry): { payer: UserBrief; receiver: UserBrief } {
  const [o] = entry.obligations
  return { payer: o.creditor, receiver: o.debtor }
}

export const isGoods = (entry: PaymentEntry): boolean => entry.item !== ''
