import { Plus, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { PeoplePicker } from '@/features/ledger/expense/PeoplePicker'
import { newRow, type Row } from '@/features/ledger/expense/rows'
import { useMoney, who } from '@/features/ledger/format'
import type { UserBrief } from '@/lib/api-types'

interface ItemsEditorProps {
  rows: Row[]
  onRows: (rows: Row[]) => void
  people: UserBrief[]
  myId: number
  /** Who a new first line starts with. */
  fallbackPeople: Set<number>
  /** Each person's total over all lines, as the server will compute it. */
  owed: Map<number, number>
  currency: string
}

/** A receipt: lines with a name and a price, each split evenly among the people ticked under it,
 * and below them what that comes to per person. */
export function ItemsEditor({
  rows,
  onRows,
  people,
  myId,
  fallbackPeople,
  owed,
  currency,
}: ItemsEditorProps) {
  const money = useMoney()
  const exponent = money.exponent(currency)
  const update = (key: number, change: Partial<Row>) =>
    onRows(rows.map((row) => (row.key === key ? { ...row, ...change } : row)))

  return (
    <>
      <ul className="flex flex-col gap-2">
        {rows.map((row, i) => (
          <li key={row.key} className="flex flex-col gap-2 rounded-lg border bg-card p-3">
            <div className="flex gap-2">
              <Input
                aria-label={`Pozycja ${i + 1}`}
                placeholder="np. Piwo"
                maxLength={100}
                value={row.name}
                onChange={(e) => update(row.key, { name: e.target.value })}
              />
              <Input
                aria-label={`Cena pozycji ${i + 1}`}
                className="w-28 shrink-0 text-right tabular-nums"
                inputMode="decimal"
                placeholder={exponent ? '0,00' : '0'}
                value={row.price}
                onChange={(e) => update(row.key, { price: e.target.value })}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="shrink-0"
                aria-label={`Usuń pozycję ${i + 1}`}
                onClick={() => onRows(rows.filter((r) => r.key !== row.key))}
              >
                <X />
              </Button>
            </div>
            <PeoplePicker
              compact
              people={people}
              selected={row.people}
              onChange={(next) => update(row.key, { people: next, custom: null })}
              label={`Kto ma udział w pozycji ${i + 1}`}
            />
            {row.custom && (
              <span className="text-xs text-muted-foreground">
                Własny podział ({row.custom.split === 'shares' ? 'udziałami' : 'kwotami'}). Zmiana
                osób przywróci podział po równo.
              </span>
            )}
          </li>
        ))}
      </ul>
      <Button
        type="button"
        variant="outline"
        className="self-start"
        onClick={() => onRows([...rows, newRow(rows.at(-1)?.people ?? fallbackPeople)])}
      >
        <Plus aria-hidden /> Dodaj pozycję
      </Button>

      {owed.size > 0 && (
        <ul className="flex flex-col divide-y rounded-lg border bg-card px-3" aria-label="Kto ile">
          {people
            .filter((p) => owed.has(p.id))
            .map((p) => (
              <li key={p.id} className="flex items-center gap-3 py-2">
                <UserAvatar username={p.username} src={p.avatar_url} size="sm" />
                <span className="min-w-0 flex-1 truncate text-base">{who(p, myId)}</span>
                <span className="text-base font-semibold tabular-nums">
                  {money.format(owed.get(p.id) ?? 0, currency)}
                </span>
              </li>
            ))}
        </ul>
      )}
    </>
  )
}
