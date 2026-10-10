import { allocate, owedFor, parseWeight } from '@/features/ledger/expense/split'
import type { ExpenseItem, ExpenseShare, SplitKind } from '@/lib/api-types'
import { parseMoney, toMoneyInput } from '@/lib/money'

/** A receipt line as typed. `people` is who is in when it is split evenly; for `shares` and
 * `exact`, `weights` holds the shares or amounts as typed (blank or zero: not in it). */
export interface Row {
  key: number
  name: string
  price: string
  split: SplitKind
  people: Set<number>
  weights: Map<number, string>
}

let nextKey = 0

export const newRow = (people: Iterable<number>, fill: Partial<Row> = {}): Row => ({
  key: nextKey++,
  name: '',
  price: '',
  split: 'equal',
  people: new Set(people),
  weights: new Map(),
  ...fill,
})

/** Everyone in, one share each: an equal split. */
export const sharesOf = (people: Iterable<number>): ExpenseShare[] =>
  [...people].sort((a, b) => a - b).map((id) => ({ user_id: id, weight: 1 }))

/** A saved item as a form row. */
export function rowOf(item: ExpenseItem, exponent: number): Row {
  const ids = item.shares.map((s) => s.user_id)
  return newRow(ids, {
    name: item.name,
    price: toMoneyInput(item.amount, exponent),
    split: item.split,
    weights:
      item.split === 'equal'
        ? new Map()
        : new Map(
            item.shares.map((s) => [
              s.user_id,
              item.split === 'exact' ? toMoneyInput(s.weight, exponent) : String(s.weight),
            ]),
          ),
  })
}

/** Who gets what share of a line, in the API's shape. */
export function rowShares(row: Row, exponent: number): ExpenseShare[] {
  if (row.split === 'equal') return sharesOf(row.people)
  return [...row.weights]
    .flatMap(([id, text]) => {
      const weight = row.split === 'shares' ? parseWeight(text) : (parseMoney(text, exponent) ?? 0)
      return weight ? [{ user_id: id, weight }] : []
    })
    .sort((a, b) => a.user_id - b.user_id)
}

/** A row as the API's item. */
export const rowItem = (row: Row, exponent: number): ExpenseItem => ({
  name: row.name.trim(),
  amount: parseMoney(row.price, exponent) ?? 0,
  split: row.split,
  shares: rowShares(row, exponent),
})

/** Who is in a line, whichever way it is split. */
export const rowMembers = (row: Row, exponent: number): Set<number> =>
  new Set(rowShares(row, exponent).map((s) => s.user_id))

/** Change how a line is divided, keeping the same people: shares start at one each, exact
 * amounts start at what each person owes now. */
export function withSplit(row: Row, split: SplitKind, exponent: number): Row {
  if (split === row.split) return row
  const item = rowItem(row, exponent)
  const members = [...rowMembers(row, exponent)].sort((a, b) => a - b)
  if (split === 'equal') return { ...row, split, people: new Set(members) }
  if (split === 'shares') {
    return { ...row, split, weights: new Map(members.map((id) => [id, '1'])) }
  }
  const owed = owedFor([item])
  const fallback = allocate(
    item.amount,
    members.map(() => 1),
  )
  return {
    ...row,
    split,
    weights: new Map(
      members.map((id, i) => [id, toMoneyInput(owed.get(id) ?? fallback[i] ?? 0, exponent)]),
    ),
  }
}

/** Put people in or out of a line, keeping everyone else's shares or amounts. Someone added to
 * shares starts with one; to exact amounts, with whatever is still unassigned (split evenly). */
export function withMembers(row: Row, next: ReadonlySet<number>, exponent: number): Row {
  if (row.split === 'equal') return { ...row, people: new Set(next) }
  const kept = new Map([...row.weights].filter(([id]) => next.has(id)))
  const added = [...next].filter((id) => !kept.has(id)).sort((a, b) => a - b)
  if (row.split === 'shares') {
    for (const id of added) kept.set(id, '1')
  } else {
    const assigned = [...kept.values()].reduce((sum, t) => sum + (parseMoney(t, exponent) ?? 0), 0)
    const left = Math.max((parseMoney(row.price, exponent) ?? 0) - assigned, 0)
    const parts = allocate(
      left,
      added.map(() => 1),
    )
    added.forEach((id, i) => kept.set(id, parts[i] ? toMoneyInput(parts[i], exponent) : ''))
  }
  return { ...row, weights: kept }
}
