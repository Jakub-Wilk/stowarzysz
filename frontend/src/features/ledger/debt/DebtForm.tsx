import { Minus, Plus } from 'lucide-react'
import { useState, type SubmitEvent } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { AmountField } from '@/features/ledger/form/AmountField'
import { DateField, FormProblem, FormSection, Segmented } from '@/features/ledger/form/controls'
import { Extras } from '@/features/ledger/form/Extras'
import { PersonSelect } from '@/features/ledger/form/PersonSelect'
import { todayInput, useMoney } from '@/features/ledger/format'
import { MutationError } from '@/features/pacts/MutationError'
import type { DebtEntry, DebtInput, LedgerMeta, UserBrief } from '@/lib/api-types'
import { formatDebt, parseMoney, toMoneyInput } from '@/lib/money'

type Direction = 'owed' | 'owe'
type What = 'money' | 'goods'

const DIRECTIONS: [Direction, string][] = [
  ['owed', 'Należy mi się'],
  ['owe', 'Jestem winien(-na)'],
]
const WHATS: [What, string][] = [
  ['money', 'Pieniądze'],
  ['goods', 'Coś innego'],
]
const MAX_GOODS = 1000 // as on the server

interface DebtFormProps {
  meta: LedgerMeta
  /** Members (plus, when editing, the other party if they have left). */
  people: UserBrief[]
  myId: number
  /** The debt being edited (one you are owed); a new one otherwise. */
  entry?: DebtEntry
  pending: boolean
  pendingLabel?: string
  error: unknown
  onSubmit: (input: DebtInput, photos: File[]) => void
}

/** A debt between you and one other member, either way round: money (in any currency) or
 * something else, "2 × piwo". It counts at once; only the person owed can change it later. */
export function DebtForm({
  meta,
  people,
  myId,
  entry,
  pending,
  pendingLabel,
  error,
  onSubmit,
}: DebtFormProps) {
  const money = useMoney()
  const start = entry?.details.debts[0]
  const [direction, setDirection] = useState<Direction>(
    start && start.debtor_id === myId ? 'owe' : 'owed',
  )
  const [other, setOther] = useState<number | null>(
    start ? (start.debtor_id === myId ? start.creditor_id : start.debtor_id) : null,
  )
  const [what, setWhat] = useState<What>(start?.item ? 'goods' : 'money')
  const [currency, setCurrency] = useState(entry?.currency || meta.base_currency)
  const [amount, setAmount] = useState(
    start && !start.item ? toMoneyInput(start.amount, money.exponent(entry?.currency ?? '')) : '',
  )
  const [item, setItem] = useState(start?.item ?? '')
  const [quantity, setQuantity] = useState(start?.item ? start.amount : 1)
  const [title, setTitle] = useState(
    entry && entry.title !== start?.item && entry.title !== 'Dług' ? entry.title : '',
  )
  const [date, setDate] = useState(entry?.occurred_on ?? todayInput())
  const [note, setNote] = useState(entry?.note ?? '')
  const [photos, setPhotos] = useState<File[]>([])
  const [localError, setLocalError] = useState<string | null>(null)

  const minor = parseMoney(amount, money.exponent(currency)) ?? 0
  const owed = direction === 'owed'
  const others = people.filter((p) => p.id !== myId)

  const problem = (): string | null => {
    if (other === null)
      return owed ? 'Wybierz, kto jest Ci winien.' : 'Wybierz, komu jesteś winien(-na).'
    if (what === 'money' && minor === 0) return 'Podaj poprawną kwotę, np. 10 lub 12,50.'
    if (what === 'goods' && !item.trim()) return 'Podaj, co jest do oddania, np. piwo.'
    return null
  }

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const message = problem()
    setLocalError(message)
    if (message || other === null) return
    onSubmit(
      {
        kind: 'debt',
        debtor_id: owed ? other : myId,
        creditor_id: owed ? myId : other,
        title: title.trim(),
        item: what === 'goods' ? item.trim() : '',
        amount: what === 'goods' ? quantity : minor,
        currency: what === 'money' ? currency : meta.base_currency,
        occurred_on: date,
        note: note.trim(),
      },
      photos,
    )
  }

  const shown =
    what === 'money'
      ? minor
        ? money.format(minor, currency)
        : ''
      : item.trim()
        ? formatDebt({ amount: quantity, item: item.trim() })
        : ''
  const label = entry ? 'Zapisz zmiany' : 'Zapisz dług'

  return (
    <form onSubmit={submit} className="flex max-w-md flex-col gap-8">
      <Segmented label="Kto komu" options={DIRECTIONS} value={direction} onChange={setDirection} />

      <FormSection title={owed ? 'Kto jest Ci winien' : 'Komu jesteś winien(-na)'}>
        <PersonSelect
          aria-label={owed ? 'Kto jest Ci winien' : 'Komu jesteś winien(-na)'}
          people={others}
          myId={myId}
          value={other}
          onChange={setOther}
        />
      </FormSection>

      <FormSection title="Co">
        <Segmented label="Co" size="sm" options={WHATS} value={what} onChange={setWhat} />
        {what === 'money' ? (
          <AmountField
            id="debt-amount"
            meta={meta}
            currency={currency}
            onCurrency={setCurrency}
            date={date}
            amount={amount}
            onAmount={setAmount}
            total={minor}
          />
        ) : (
          <div className="flex items-center gap-2">
            <Input
              aria-label="Co jest do oddania"
              maxLength={60}
              placeholder="np. piwo, kolacja"
              value={item}
              onChange={(e) => setItem(e.target.value)}
            />
            <span className="flex shrink-0 items-center gap-1" role="group" aria-label="Ile sztuk">
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="Mniej"
                disabled={quantity <= 1}
                onClick={() => setQuantity(quantity - 1)}
              >
                <Minus />
              </Button>
              <span
                className="w-8 text-center text-lg font-semibold tabular-nums"
                aria-live="polite"
              >
                {quantity}
              </span>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="Więcej"
                disabled={quantity >= MAX_GOODS}
                onClick={() => setQuantity(quantity + 1)}
              >
                <Plus />
              </Button>
            </span>
          </div>
        )}
      </FormSection>

      <div className="flex flex-col gap-3">
        <Input
          aria-label="Za co"
          maxLength={200}
          placeholder="Za co? (opcjonalnie)"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <DateField id="debt-date" value={date} onChange={setDate} />
      </div>

      <Extras
        note={note}
        onNote={setNote}
        photos={photos}
        onPhotos={setPhotos}
        entry={entry}
        myId={myId}
      />

      <div className="flex flex-col gap-3">
        <FormProblem>{localError}</FormProblem>
        <MutationError error={error} />
        <Button type="submit" size="lg" disabled={pending}>
          {pending ? (pendingLabel ?? 'Zapisywanie…') : shown ? `${label} · ${shown}` : label}
        </Button>
      </div>
    </form>
  )
}
