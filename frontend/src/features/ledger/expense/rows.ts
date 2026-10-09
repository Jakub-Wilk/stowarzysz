import type { ExpenseItem, ExpenseShare, SplitKind } from '@/lib/api-types'

/** A receipt line as typed. A custom split (made through the API) is kept until its people change. */
export interface Row {
  key: number
  name: string
  price: string
  people: Set<number>
  custom: { split: SplitKind; shares: ExpenseItem['shares'] } | null
}

let nextKey = 0

export const newRow = (people: Iterable<number>, fill: Partial<Row> = {}): Row => ({
  key: nextKey++,
  name: '',
  price: '',
  people: new Set(people),
  custom: null,
  ...fill,
})

/** Everyone in, one share each: an equal split. */
export const sharesOf = (people: Iterable<number>): ExpenseShare[] =>
  [...people].sort((a, b) => a - b).map((id) => ({ user_id: id, weight: 1 }))
