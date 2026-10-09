import type { LedgerMeta } from '@/lib/api-types'

/** A category's chart colour, fixed by its place in the list (and `debts` after them), so it
 * is the same in every chart and in every theme. */
export function categoryColor(key: string, meta: LedgerMeta | undefined): string {
  const keys = [...(meta?.categories ?? []).map((c) => c.key), 'debts']
  const index = Math.max(keys.indexOf(key as never), 0)
  return `oklch(0.72 0.14 ${(index * 360) / keys.length + 20})`
}
