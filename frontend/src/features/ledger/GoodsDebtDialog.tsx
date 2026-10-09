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
import { usePeople } from '@/features/voting/hooks'

/** Write down that somebody owes you something that is not money, e.g. "2 × piwo". Money owed
 * to you is an expense you paid for them. */
export function GoodsDebtDialog({
  myId,
  open,
  onOpenChange,
}: {
  myId: number
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const people = usePeople()
  const create = useCreateEntry()
  const [debtorId, setDebtorId] = useState('')
  const [item, setItem] = useState('')
  const [quantity, setQuantity] = useState('1')
  const [note, setNote] = useState('')
  const [localError, setLocalError] = useState<string | null>(null)

  const submit = () => {
    setLocalError(null)
    const amount = parseQuantity(quantity)
    if (!debtorId) return setLocalError('Wybierz, kto jest Ci winien.')
    if (!item.trim()) return setLocalError('Podaj, co jest winien, np. piwo.')
    if (amount === null) return setLocalError('Podaj poprawną liczbę sztuk.')
    create.mutate(
      { kind: 'debt', debtor_id: Number(debtorId), item, amount, note },
      {
        onSuccess: () => {
          setItem('')
          setQuantity('1')
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
          create.reset()
        }
        onOpenChange(next)
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ktoś jest Ci winien</DialogTitle>
          <DialogDescription>
            Piwo, kolacja, przysługa… Wpis liczy się od razu, a oddanie potwierdzasz Ty.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <Label htmlFor="debt-debtor">Kto jest winien</Label>
          <select
            id="debt-debtor"
            className="h-12 w-full rounded-lg border border-input bg-transparent px-3 text-base outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
            value={debtorId}
            onChange={(e) => setDebtorId(e.target.value)}
          >
            <option value="">Wybierz…</option>
            {(people.data ?? [])
              .filter((p) => p.id !== myId)
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.username}
                </option>
              ))}
          </select>
        </div>
        <div className="flex gap-2">
          <div className="flex flex-1 flex-col gap-2">
            <Label htmlFor="debt-item">Co</Label>
            <Input
              id="debt-item"
              maxLength={60}
              placeholder="np. piwo"
              value={item}
              onChange={(e) => setItem(e.target.value)}
            />
          </div>
          <div className="flex w-24 flex-col gap-2">
            <Label htmlFor="debt-quantity">Ile</Label>
            <Input
              id="debt-quantity"
              inputMode="numeric"
              value={quantity}
              onChange={(e) => setQuantity(e.target.value)}
            />
          </div>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="debt-note">Za co (opcjonalnie)</Label>
          <Input
            id="debt-note"
            maxLength={500}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </div>
        {localError && (
          <span role="alert" className="text-base text-destructive">
            {localError}
          </span>
        )}
        <MutationError error={create.error} />
        <DialogFooter>
          <Button disabled={create.isPending} onClick={submit}>
            {create.isPending ? 'Zapisywanie…' : 'Zapisz dług'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
