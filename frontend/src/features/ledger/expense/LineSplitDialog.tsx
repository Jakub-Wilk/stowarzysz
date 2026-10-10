import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { SplitEditor } from '@/features/ledger/expense/SplitEditor'
import {
  rowItem,
  rowMembers,
  withMembers,
  withSplit,
  type Row,
} from '@/features/ledger/expense/rows'
import { owedFor } from '@/features/ledger/expense/split'
import { Segmented } from '@/features/ledger/form/controls'
import { useMoney } from '@/features/ledger/format'
import type { SplitKind, UserBrief } from '@/lib/api-types'

const SPLITS: [SplitKind, string][] = [
  ['equal', 'Po równo'],
  ['shares', 'Udziały'],
  ['exact', 'Kwoty'],
]

interface LineSplitDialogProps {
  row: Row
  open: boolean
  onOpenChange: (open: boolean) => void
  onChange: (row: Row) => void
  /** Give every other line this line's way of dividing (not offered for exact amounts). */
  onApplyToAll: () => void
  people: UserBrief[]
  myId: number
  currency: string
}

/** How one receipt line is divided: evenly, by shares or in exact amounts. Edits apply live. */
export function LineSplitDialog({
  row,
  open,
  onOpenChange,
  onChange,
  onApplyToAll,
  people,
  myId,
  currency,
}: LineSplitDialogProps) {
  const money = useMoney()
  const exponent = money.exponent(currency)
  const item = rowItem(row, exponent)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            Podział: {row.name.trim() || 'pozycja'}
            {item.amount > 0 && ` · ${money.format(item.amount, currency)}`}
          </DialogTitle>
        </DialogHeader>
        <Segmented
          label="Sposób podziału pozycji"
          size="sm"
          options={SPLITS}
          value={row.split}
          onChange={(split) => onChange(withSplit(row, split, exponent))}
        />
        <SplitEditor
          people={people}
          myId={myId}
          split={row.split}
          sharedBy={row.people}
          onSharedBy={(next) => onChange(withMembers(row, next, exponent))}
          weights={row.weights}
          onWeights={(weights) => onChange({ ...row, weights })}
          owed={owedFor([item])}
          total={item.amount}
          currency={currency}
        />
        <DialogFooter>
          {row.split !== 'exact' && rowMembers(row, exponent).size > 0 && (
            <Button type="button" variant="ghost" onClick={onApplyToAll}>
              Zastosuj do wszystkich pozycji
            </Button>
          )}
          <Button type="button" onClick={() => onOpenChange(false)}>
            Gotowe
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
