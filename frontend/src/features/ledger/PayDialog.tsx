import { useState } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { parseQuantity } from '@/features/ledger/format'
import { useCreateEntry } from '@/features/ledger/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import type { LedgerTransfer } from '@/lib/api-types'
import { parseMoney, toMoneyInput } from '@/lib/money'

/** Pay off a suggested transfer (the whole amount to begin with). It counts once confirmed. */
export function PayDialog({
  transfer,
  open,
  onOpenChange,
}: {
  transfer: LedgerTransfer
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const pay = useCreateEntry()
  const goods = transfer.item !== ''
  const receiver = transfer.creditor.username
  const [amount, setAmount] = useState(() =>
    goods ? String(transfer.amount) : toMoneyInput(transfer.amount),
  )
  const [note, setNote] = useState('')
  const [localError, setLocalError] = useState<string | null>(null)

  const submit = () => {
    setLocalError(null)
    const value = goods ? parseQuantity(amount) : parseMoney(amount)
    if (value === null) {
      return setLocalError(
        goods ? 'Podaj poprawną liczbę sztuk.' : 'Podaj poprawną kwotę, np. 10 lub 12,50.',
      )
    }
    pay.mutate(
      {
        kind: 'payment',
        to_user_id: transfer.creditor.id,
        amount: value,
        item: transfer.item,
        note,
      },
      {
        onSuccess: () => {
          setNote('')
          onOpenChange(false)
        },
      },
    )
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setLocalError(null)
          pay.reset()
        }
        onOpenChange(next)
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {goods ? 'Oddaj' : 'Zapłać'} {receiver}
          </DialogTitle>
          <DialogDescription>
            {goods ? 'Zwrot' : 'Wpłata'} liczy się dopiero wtedy, gdy {receiver}{' '}
            {goods ? 'go' : 'ją'} potwierdzi.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <Label htmlFor={`pay-amount-${transfer.creditor.id}`}>
            {goods ? `Ile sztuk (${transfer.item})` : 'Kwota (zł)'}
          </Label>
          <Input
            id={`pay-amount-${transfer.creditor.id}`}
            inputMode={goods ? 'numeric' : 'decimal'}
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor={`pay-note-${transfer.creditor.id}`}>Notatka (opcjonalnie)</Label>
          <Input
            id={`pay-note-${transfer.creditor.id}`}
            maxLength={500}
            placeholder={goods ? 'np. oddane w piątek' : 'np. BLIK, gotówka'}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </div>
        {localError && (
          <span role="alert" className="text-base text-destructive">
            {localError}
          </span>
        )}
        <MutationError error={pay.error} />
        <DialogFooter>
          <Button disabled={pay.isPending} onClick={submit}>
            {pay.isPending ? 'Wysyłanie…' : 'Wyślij do potwierdzenia'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
