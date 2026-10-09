/**
 * The expense form's live preview of who owes what. It mirrors `backend/ledger` exactly, so
 * the preview matches what will be saved, but the server stays the authority.
 */
import type { ExpenseItem } from '@/lib/api-types'

/** Mirrors `ledger.money.allocate`: split `total` by `weights` in whole units, summing exactly
 * (largest remainder; ties go to the earlier entry). */
export function allocate(total: number, weights: number[]): number[] {
  const sum = weights.reduce((a, b) => a + b, 0)
  if (total === 0 || sum === 0) return weights.map(() => 0)
  const shares = weights.map((w) => Math.floor((total * w) / sum))
  let leftover = total - shares.reduce((a, b) => a + b, 0)
  const byRemainder = weights
    .map((w, i) => ({ i, remainder: (total * w) % sum }))
    .sort((a, b) => b.remainder - a.remainder || a.i - b.i)
  for (const { i } of byRemainder) {
    if (leftover-- <= 0) break
    shares[i] += 1
  }
  return shares
}

/** Mirrors `ExpenseKind._owed`: each item divided among its sharers, the odd grosz taking turns
 * from item to item. Returns what each person owes in total. */
export function owedFor(items: ExpenseItem[]): Map<number, number> {
  const owed = new Map<number, number>()
  items.forEach((item, position) => {
    if (item.shares.length === 0) return
    const weights = new Map(item.shares.map((s) => [s.user_id, s.weight]))
    const ids = [...weights.keys()].sort((a, b) => a - b)
    const turn = position % ids.length
    const rotated = [...ids.slice(turn), ...ids.slice(0, turn)]
    const split =
      item.split === 'equal' ? rotated.map(() => 1) : rotated.map((u) => weights.get(u) ?? 0)
    allocate(item.amount, split).forEach((amount, i) => {
      owed.set(rotated[i], (owed.get(rotated[i]) ?? 0) + amount)
    })
  })
  return owed
}

/** A number of shares as typed: "2" -> 2; 0 for anything that is not a whole number above 0. */
export const parseWeight = (text: string): number =>
  /^\d+$/.test(text.trim()) && Number(text) > 0 ? Number(text) : 0
