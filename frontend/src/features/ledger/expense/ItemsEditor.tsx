import { ChevronDown, Plus, X } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { LineSplitDialog } from '@/features/ledger/expense/LineSplitDialog'
import { PeoplePicker } from '@/features/ledger/expense/PeoplePicker'
import { newRow, rowItem, rowMembers, withMembers, type Row } from '@/features/ledger/expense/rows'
import { useMoney, who } from '@/features/ledger/format'
import type { UserBrief } from '@/lib/api-types'
import { cn } from '@/lib/utils'

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

/** A receipt: lines with a name and a price, each split among the people ticked under it (evenly,
 * or by shares or exact amounts, set in a dialog), and below them what that comes to per person. */
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
  const [editing, setEditing] = useState<number | null>(null)
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
            <LineSplit
              row={row}
              exponent={exponent}
              currency={currency}
              people={people}
              onOpen={() => setEditing(row.key)}
              onMembers={(next) =>
                onRows(rows.map((r) => (r.key === row.key ? withMembers(r, next, exponent) : r)))
              }
              label={`Kto ma udział w pozycji ${i + 1}`}
            />
          </li>
        ))}
      </ul>
      {rows.map((row) => (
        <LineSplitDialog
          key={row.key}
          row={row}
          open={editing === row.key}
          onOpenChange={(open) => setEditing(open ? row.key : null)}
          onChange={(next) => onRows(rows.map((r) => (r.key === row.key ? next : r)))}
          onApplyToAll={() => {
            const members = rowMembers(row, exponent)
            onRows(
              rows.map((r) =>
                r.key === row.key
                  ? r
                  : {
                      ...r,
                      split: row.split,
                      people: new Set(members),
                      weights: new Map(row.weights),
                    },
              ),
            )
            setEditing(null)
          }}
          people={people}
          myId={myId}
          currency={currency}
        />
      ))}
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

interface LineSplitProps {
  row: Row
  exponent: number
  currency: string
  people: UserBrief[]
  onOpen: () => void
  onMembers: (next: Set<number>) => void
  label: string
}

/** A line's people (with their shares or amounts beside them) and a button that says how it is
 * divided and opens the dialog to change that. */
function LineSplit({ row, exponent, currency, people, onOpen, onMembers, label }: LineSplitProps) {
  const money = useMoney()
  const item = rowItem(row, exponent)
  const assigned = item.shares.reduce((sum, s) => sum + s.weight, 0)
  const off = row.split === 'exact' ? item.amount - assigned : 0
  const badges = new Map(
    row.split === 'equal'
      ? []
      : item.shares.map((s) => [
          s.user_id,
          row.split === 'shares' ? `×${s.weight}` : money.format(s.weight, currency),
        ]),
  )
  const summary =
    row.split === 'equal'
      ? 'Po równo'
      : row.split === 'shares'
        ? `Udziały · ${item.shares.map((s) => s.weight).join(':')}`
        : off > 0
          ? `Kwoty · brakuje ${money.format(off, currency)}`
          : off < 0
            ? `Kwoty · o ${money.format(-off, currency)} za dużo`
            : 'Kwoty'

  return (
    <>
      <PeoplePicker
        compact
        hideAll={row.split === 'exact'}
        people={people}
        selected={rowMembers(row, exponent)}
        onChange={onMembers}
        badges={badges}
        label={label}
      />
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className={cn('-ml-2 self-start', off !== 0 && 'text-destructive')}
        onClick={onOpen}
      >
        {summary}
        <ChevronDown data-icon="inline-end" aria-hidden />
      </Button>
    </>
  )
}
