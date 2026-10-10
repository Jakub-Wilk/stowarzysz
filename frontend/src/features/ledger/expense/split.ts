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

const gcd = (a: bigint, b: bigint): bigint => (b === 0n ? a : gcd(b, a % b))

/** Mirrors `ledger.money.split_items`: each item divided among its sharers, the odd grosze going
 * to whoever is furthest behind their exact share so far on the whole receipt (ties: lower user
 * id), so rounding evens out from item to item. Returns what each person owes in total. BigInt
 * keeps the bookkeeping exact (errors are counted in 1/scale grosz). */
export function owedFor(items: ExpenseItem[]): Map<number, number> {
  const sumOf = (item: ExpenseItem) => BigInt(item.shares.reduce((sum, s) => sum + s.weight, 0))
  let scale = 1n
  for (const item of items) {
    const sum = sumOf(item)
    if (sum > 0n) scale = (scale / gcd(scale, sum)) * sum
  }
  const behind = new Map<number, bigint>() // assigned minus exact share, times scale
  const owed = new Map<number, number>()
  for (const item of items) {
    const totalWeight = sumOf(item)
    if (totalWeight === 0n) continue
    const amount = BigInt(item.amount)
    const unit = scale / totalWeight
    const people = [...item.shares].sort((a, b) => a.user_id - b.user_id)
    const shares = new Map(
      people.map((s) => [s.user_id, (amount * BigInt(s.weight)) / totalWeight] as const),
    )
    let leftover = Number(amount) - [...shares.values()].reduce((sum, v) => sum + Number(v), 0)
    const error = new Map<number, bigint>()
    for (const { user_id: id, weight } of people) {
      const exact = amount * BigInt(weight)
      if (exact % totalWeight === 0n) continue
      const given = (shares.get(id) ?? 0n) * totalWeight
      error.set(id, (behind.get(id) ?? 0n) + (given - exact) * unit)
    }
    const order = [...error].sort(([ia, ea], [ib, eb]) => (ea === eb ? ia - ib : ea < eb ? -1 : 1))
    for (const [id, err] of order) {
      if (leftover-- <= 0) break
      shares.set(id, (shares.get(id) ?? 0n) + 1n)
      error.set(id, err + scale)
    }
    for (const [id, share] of shares) {
      owed.set(id, (owed.get(id) ?? 0) + Number(share))
      behind.set(id, error.get(id) ?? behind.get(id) ?? 0n)
    }
  }
  return owed
}

/** A number of shares as typed: "2" -> 2; 0 for anything that is not a whole number above 0. */
export const parseWeight = (text: string): number =>
  /^\d+$/.test(text.trim()) && Number(text) > 0 ? Number(text) : 0
