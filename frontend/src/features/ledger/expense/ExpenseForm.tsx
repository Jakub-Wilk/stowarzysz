import { Camera, LoaderCircle } from 'lucide-react'
import { useRef, useState, type SubmitEvent } from 'react'

import { SelectField } from '@/components/SelectField'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ItemsEditor } from '@/features/ledger/expense/ItemsEditor'
import { PayersField } from '@/features/ledger/expense/PayersField'
import { newRow, rowItem, rowOf, type Row, sharesOf } from '@/features/ledger/expense/rows'
import { ScanProgress } from '@/features/ledger/expense/ScanProgress'
import { scannedCurrency, scannedRows } from '@/features/ledger/expense/scannedRows'
import { useReceiptScan } from '@/features/ledger/expense/useReceiptScan'
import { allocate, owedFor, parseWeight } from '@/features/ledger/expense/split'
import { SplitEditor } from '@/features/ledger/expense/SplitEditor'
import { WORDING } from '@/features/ledger/expense/wording'
import { AmountField } from '@/features/ledger/form/AmountField'
import { DateField, FormProblem, FormSection, Segmented } from '@/features/ledger/form/controls'
import { Extras } from '@/features/ledger/form/Extras'
import { todayInput, useMoney } from '@/features/ledger/format'
import { MAX_PHOTOS } from '@/features/ledger/hooks'
import { isItemized } from '@/features/ledger/kinds/helpers'
import { ApiError } from '@/lib/api'
import { MutationError } from '@/features/pacts/MutationError'
import type {
  ExpenseInput,
  ExpenseItem,
  LedgerCategory,
  LedgerMeta,
  SharedEntry,
  SharedKindKey,
  SplitKind,
  UserBrief,
} from '@/lib/api-types'
import { parseMoney, toMoneyInput } from '@/lib/money'

/** How it is divided: one total split evenly, by shares or into exact amounts, or a receipt
 * whose every line is split evenly among its own people. */
type Method = SplitKind | 'items'

const METHODS: [Method, string][] = [
  ['equal', 'Po równo'],
  ['shares', 'Udziałami'],
  ['exact', 'Kwotami'],
  ['items', 'Pozycjami'],
]

const KINDS: [SharedKindKey, string][] = [
  ['expense', 'Wydatek'],
  ['income', 'Przychód'],
]

interface ExpenseFormProps {
  /** An expense, or an income (the same form, worded the other way round). */
  kind: SharedKindKey
  /** Switch between the two (a new entry only). */
  onKind?: (kind: SharedKindKey) => void
  meta: LedgerMeta
  /** Members (plus, when editing, anyone the entry mentions). */
  people: UserBrief[]
  myId: number
  /** The entry being edited; a new one otherwise. */
  entry?: SharedEntry
  pending: boolean
  /** What the button says while saving ("Wysyłanie zdjęć 1/2…"). */
  pendingLabel?: string
  error: unknown
  /** The entry, plus photos to upload once it is saved. */
  onSubmit: (input: ExpenseInput, photos: File[]) => void
}

/**
 * A shared expense (or income), Tricount-style, in the order people think of it: how much, for
 * what, who paid, and how it is divided, with each person's part shown live beside them.
 */
export function ExpenseForm({
  kind,
  onKind,
  meta,
  people,
  myId,
  entry,
  pending,
  pendingLabel,
  error,
  onSubmit,
}: ExpenseFormProps) {
  const words = WORDING[kind]
  const money = useMoney()
  const startExponent = money.exponent(entry?.currency ?? meta.base_currency)
  const asInput = (minor: number) => toMoneyInput(minor, startExponent)
  const startItems = entry?.details.items ?? []
  const startPayers = entry?.details.payers ?? []
  // a single total is one item named like the entry; anything else is a receipt
  const single = entry && !isItemized(entry) ? startItems[0] : null

  const [title, setTitle] = useState(entry?.title ?? '')
  const [currency, setCurrency] = useState(entry?.currency ?? meta.base_currency)
  const [date, setDate] = useState(entry?.occurred_on ?? todayInput())
  const [category, setCategory] = useState<LedgerCategory>(
    entry?.category || (kind === 'income' ? 'other' : 'food'),
  )
  const [note, setNote] = useState(entry?.note ?? '')
  const [photos, setPhotos] = useState<File[]>([])
  const [method, setMethod] = useState<Method>(
    entry && !single ? 'items' : (single?.split ?? 'equal'),
  )
  const [amount, setAmount] = useState(entry && single ? asInput(single.amount) : '')
  const [sharedBy, setSharedBy] = useState<Set<number>>(
    () => new Set(single ? single.shares.map((s) => s.user_id) : people.map((p) => p.id)),
  )
  const [weights, setWeights] = useState<Map<number, string>>(
    () =>
      new Map(
        single && single.split !== 'equal'
          ? single.shares.map((s) => [
              s.user_id,
              single.split === 'exact' ? asInput(s.weight) : String(s.weight),
            ])
          : [],
      ),
  )
  const [rows, setRows] = useState<Row[]>(() =>
    single ? [] : startItems.map((item) => rowOf(item, startExponent)),
  )
  const [payer, setPayer] = useState<number | null>(
    startPayers.length === 1 ? startPayers[0].user_id : myId,
  )
  const [payers, setPayers] = useState<Map<number, string> | null>(() =>
    startPayers.length > 1
      ? new Map(startPayers.map((p) => [p.user_id, asInput(p.amount)] as const))
      : null,
  )
  const [localError, setLocalError] = useState<string | null>(null)
  const receiptScan = useReceiptScan()
  const [scanError, setScanError] = useState<unknown>(null)
  const scanInput = useRef<HTMLInputElement>(null)
  const exponent = money.exponent(currency)
  const split: SplitKind = method === 'items' ? 'equal' : method

  /** Shares or exact amounts as typed (blank or zero: not in it). */
  const typedShares = [...weights]
    .flatMap(([id, text]) => {
      const weight = split === 'shares' ? parseWeight(text) : (parseMoney(text, exponent) ?? 0)
      return weight ? [{ user_id: id, weight }] : []
    })
    .sort((a, b) => a.user_id - b.user_id)
  /** Who is in a single total right now, whichever way it is split. */
  const participants = (): Set<number> =>
    split === 'equal' ? sharedBy : new Set(typedShares.map((s) => s.user_id))

  // what the form says right now, in the API's shape
  const items: ExpenseItem[] =
    method === 'items'
      ? rows.map((row) => rowItem(row, exponent))
      : [
          {
            name: title.trim() || words.noun,
            amount: parseMoney(amount, exponent) ?? 0,
            split,
            shares: split === 'equal' ? sharesOf(sharedBy) : typedShares,
          },
        ]
  const total = items.reduce((sum, item) => sum + item.amount, 0)
  const owed = owedFor(items)
  const paidBy = payers
    ? [...payers].map(([id, text]) => ({ user_id: id, amount: parseMoney(text, exponent) ?? 0 }))
    : [{ user_id: payer ?? myId, amount: total }]
  const paid = paidBy.reduce((sum, p) => sum + p.amount, 0)

  /** Switch how it is divided, keeping the same people (exact amounts start even, a receipt
   * starts with the amount typed so far as its first line). */
  const changeMethod = (next: Method) => {
    if (next === method) return
    const ids = [...participants()].sort((a, b) => a - b)
    if (next === 'items') {
      if (rows.length === 0) setRows([newRow(ids, { price: amount })])
    } else {
      if (method === 'items' && !amount.trim() && total) setAmount(toMoneyInput(total, exponent))
      const base = method === 'items' ? total : (parseMoney(amount, exponent) ?? 0)
      if (next === 'equal') setSharedBy(new Set(ids))
      else if (next === 'shares') setWeights(new Map(ids.map((id) => [id, '1'])))
      else {
        const even = allocate(
          base,
          ids.map(() => 1),
        )
        setWeights(new Map(ids.map((id, i) => [id, toMoneyInput(even[i], exponent)])))
      }
    }
    setMethod(next)
  }

  /** Read a receipt photo: its lines replace the receipt, split among everyone in for now. */
  const scanReceipt = async (file: File) => {
    setScanError(null)
    try {
      const receipt = await receiptScan.scan(file)
      // the lines are in the receipt's currency: read them with its exponent, not the form's
      const receiptCurrency = scannedCurrency(receipt, meta.currencies) ?? currency
      const scanned = scannedRows(
        receipt,
        money.exponent(receiptCurrency),
        people.map((p) => p.id),
      )
      if (scanned.length === 0) {
        setScanError(
          new ApiError(422, { detail: 'Nie znaleziono na paragonie żadnych pozycji z ceną.' }),
        )
        return
      }
      setCurrency(receiptCurrency)
      setRows(scanned)
      setMethod('items')
      if (!title.trim() && receipt.merchant?.name) setTitle(receipt.merchant.name)
      setPhotos((current) => (current.length < MAX_PHOTOS ? [...current, file] : current))
    } catch (error) {
      setScanError(error)
    }
  }

  const problem = (): string | null => {
    if (method !== 'items' && total === 0) return 'Podaj poprawną kwotę, np. 10 lub 12,50.'
    if (!title.trim()) return words.titleMissing
    if (method === 'items' && rows.length === 0) return 'Dodaj co najmniej jedną pozycję.'
    if (items.some((item) => !item.name || item.amount === 0)) {
      return 'Każda pozycja potrzebuje nazwy i poprawnej ceny.'
    }
    if (items.some((item) => item.shares.length === 0)) return 'Zaznacz, kogo to dotyczy.'
    if (
      method === 'items' &&
      items.some(
        (item) =>
          item.split === 'exact' &&
          item.shares.reduce((sum, s) => sum + s.weight, 0) !== item.amount,
      )
    ) {
      return 'Kwoty osób w każdej pozycji muszą dać razem jej cenę.'
    }
    if (split === 'exact' && method !== 'items') {
      const assigned = typedShares.reduce((sum, s) => sum + s.weight, 0)
      if (assigned !== total) {
        return `Kwoty osób muszą dać razem ${money.format(total, currency)}, a jest ${money.format(assigned, currency)}.`
      }
    }
    if (paid !== total) {
      return words.payersMismatch(money.format(total, currency), money.format(paid, currency))
    }
    return null
  }

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const message = problem()
    setLocalError(message)
    if (message) return
    onSubmit(
      {
        kind,
        title: title.trim(),
        currency,
        occurred_on: date,
        category,
        note: note.trim(),
        items,
        payers: paidBy.filter((p) => p.amount > 0),
      },
      photos,
    )
  }

  const everyone = people.length > 0 && people.every((p) => sharedBy.has(p.id))
  const label = entry ? 'Zapisz zmiany' : words.submit

  return (
    <form onSubmit={submit} className="flex max-w-md flex-col gap-8">
      {onKind && <Segmented label="Rodzaj wpisu" options={KINDS} value={kind} onChange={onKind} />}

      <div className="flex flex-col gap-3">
        <div className="flex gap-2">
          <SelectField
            display="icon"
            aria-label="Kategoria"
            value={category}
            onChange={setCategory}
            options={meta.categories.map((c) => ({ value: c.key, label: c.label, icon: c.emoji }))}
          />
          <Input
            aria-label={words.titleLabel}
            autoFocus={!entry}
            value={title}
            maxLength={200}
            placeholder={words.titlePlaceholder}
            onChange={(e) => setTitle(e.target.value)}
          />
        </div>
        <DateField id="entry-date" value={date} onChange={setDate} />
      </div>

      <PayersField
        title={words.payersHeading}
        people={people}
        myId={myId}
        payer={payer}
        onPayer={setPayer}
        payers={payers}
        onPayers={setPayers}
        total={total}
        paid={paid}
        currency={currency}
      />

      <FormSection title="Ile">
        <AmountField
          id="entry-amount"
          meta={meta}
          currency={currency}
          onCurrency={setCurrency}
          date={date}
          amount={amount}
          onAmount={method === 'items' ? undefined : setAmount}
          total={total}
          caption="Suma pozycji z paragonu"
        />
      </FormSection>

      <FormSection
        title={words.partHeading}
        action={
          method === 'equal' && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setSharedBy(everyone ? new Set() : new Set(people.map((p) => p.id)))}
            >
              {everyone ? 'Nikt' : 'Wszyscy'}
            </Button>
          )
        }
      >
        <div className="flex flex-col gap-2">
          <input
            ref={scanInput}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0]
              e.target.value = ''
              if (file) void scanReceipt(file)
            }}
          />
          <Button
            type="button"
            variant="outline"
            disabled={receiptScan.pending}
            onClick={() => scanInput.current?.click()}
          >
            {receiptScan.pending ? <LoaderCircle className="animate-spin" /> : <Camera />}
            {receiptScan.pending ? 'Odczytywanie paragonu…' : 'Skanuj paragon'}
          </Button>
          {(receiptScan.pending || receiptScan.quarters > 0) && (
            <ScanProgress quarters={receiptScan.quarters} />
          )}
          <MutationError error={scanError} />
        </div>
        <Segmented
          label="Sposób podziału"
          size="sm"
          options={METHODS}
          value={method}
          onChange={changeMethod}
        />
        {method === 'items' ? (
          <ItemsEditor
            rows={rows}
            onRows={setRows}
            people={people}
            myId={myId}
            fallbackPeople={sharedBy}
            owed={owed}
            currency={currency}
          />
        ) : (
          <SplitEditor
            people={people}
            myId={myId}
            split={split}
            sharedBy={sharedBy}
            onSharedBy={setSharedBy}
            weights={weights}
            onWeights={setWeights}
            owed={owed}
            total={total}
            currency={currency}
          />
        )}
      </FormSection>

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
          {pending
            ? (pendingLabel ?? 'Zapisywanie…')
            : total
              ? `${label} · ${money.format(total, currency)}`
              : label}
        </Button>
      </div>
    </form>
  )
}
