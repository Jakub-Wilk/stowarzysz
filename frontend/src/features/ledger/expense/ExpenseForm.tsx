import { Plus, WandSparkles, X } from 'lucide-react'
import { useState, type SubmitEvent } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { PeoplePicker } from '@/features/ledger/expense/PeoplePicker'
import { allocate, owedFor } from '@/features/ledger/expense/split'
import { formatDay, todayInput, useMoney, who } from '@/features/ledger/format'
import { useRate } from '@/features/ledger/hooks'
import { isItemized } from '@/features/ledger/kinds/helpers'
import { MutationError } from '@/features/pacts/MutationError'
import type {
  ExpenseEntry,
  ExpenseInput,
  ExpenseItem,
  LedgerCategory,
  LedgerMeta,
  SplitKind,
  UserBrief,
} from '@/lib/api-types'
import { formatMoney, parseMoney, toMoneyInput } from '@/lib/money'
import { cn } from '@/lib/utils'

const SELECT =
  'h-12 w-full rounded-lg border border-input bg-transparent px-3 text-base outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30'

type Mode = 'simple' | 'items'

/** A receipt line as typed. A custom split (made through the API) is kept until its people change. */
interface Row {
  key: number
  name: string
  price: string
  people: Set<number>
  custom: { split: SplitKind; shares: ExpenseItem['shares'] } | null
}

let nextKey = 0
const newRow = (people: Iterable<number>): Row => ({
  key: nextKey++,
  name: '',
  price: '',
  people: new Set(people),
  custom: null,
})

const sharesOf = (people: Iterable<number>) => [...people].map((id) => ({ user_id: id, weight: 1 }))

/** Who pays: one person (a select), or several with their amounts. */
function PayersField({
  people,
  myId,
  payer,
  onPayer,
  payers,
  onPayers,
  total,
  paid,
  currency,
  exponent,
}: {
  people: UserBrief[]
  myId: number | undefined
  payer: number | null
  onPayer: (id: number) => void
  payers: Map<number, string> | null
  onPayers: (next: Map<number, string> | null) => void
  total: number
  paid: number
  currency: string
  exponent: number
}) {
  const money = useMoney()
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 text-base font-medium">Kto płacił</legend>
      {payers === null ? (
        <div className="flex gap-2">
          <select
            aria-label="Kto płacił"
            className={SELECT}
            value={payer ?? ''}
            onChange={(e) => onPayer(Number(e.target.value))}
          >
            {people.map((p) => (
              <option key={p.id} value={p.id}>
                {p.id === myId ? `${p.username} (Ty)` : p.username}
              </option>
            ))}
          </select>
          <Button
            type="button"
            variant="ghost"
            className="shrink-0"
            onClick={() =>
              onPayers(new Map(payer !== null ? [[payer, toMoneyInput(total, exponent)]] : []))
            }
          >
            Kilka osób
          </Button>
        </div>
      ) : (
        <>
          <ul className="flex flex-col gap-2">
            {people.map((p) => (
              <li key={p.id} className="flex items-center gap-3">
                <UserAvatar username={p.username} src={p.avatar_url} size="sm" />
                <span className="min-w-0 flex-1 truncate text-base">{who(p, myId)}</span>
                <Input
                  aria-label={`Ile zapłacił(a) ${p.username}`}
                  className="w-32"
                  inputMode="decimal"
                  placeholder="0"
                  value={payers.get(p.id) ?? ''}
                  onChange={(e) => onPayers(new Map(payers).set(p.id, e.target.value))}
                />
              </li>
            ))}
          </ul>
          <div className="flex items-center justify-between gap-2 text-sm">
            <span className={cn(paid !== total && 'text-destructive')}>
              Zapłacono {money.format(paid, currency)} z {money.format(total, currency)}
            </span>
            <Button type="button" variant="ghost" onClick={() => onPayers(null)}>
              Jedna osoba
            </Button>
          </div>
        </>
      )}
    </fieldset>
  )
}

/** The lines of a receipt, each with its own people (split evenly among them). */
function ItemsField({
  rows,
  onRows,
  people,
  fallbackPeople,
  exponent,
}: {
  rows: Row[]
  onRows: (rows: Row[]) => void
  people: UserBrief[]
  fallbackPeople: Set<number>
  exponent: number
}) {
  const update = (key: number, change: Partial<Row>) =>
    onRows(rows.map((row) => (row.key === key ? { ...row, ...change } : row)))
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1 text-base font-medium">Pozycje</legend>
      <p className="text-sm text-muted-foreground">
        Każdą pozycję dzielą po równo osoby, które przy niej zaznaczysz.
      </p>
      {rows.map((row, i) => (
        <div key={row.key} className="flex flex-col gap-2 rounded-lg border bg-card p-3">
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
              className="w-28"
              inputMode="decimal"
              placeholder={exponent ? '0,00' : '0'}
              value={row.price}
              onChange={(e) => update(row.key, { price: e.target.value })}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
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
        </div>
      ))}
      <Button
        type="button"
        variant="ghost"
        className="self-start"
        onClick={() => onRows([...rows, newRow(rows.at(-1)?.people ?? fallbackPeople)])}
      >
        <Plus /> Pozycja
      </Button>
    </fieldset>
  )
}

/** The rate a foreign-currency expense will use, fetched (and cached) while the user types. */
function RateNote({
  currency,
  date,
  total,
  exponent,
}: {
  currency: string
  date: string
  total: number
  exponent: number
}) {
  const rate = useRate(currency, date, currency !== 'PLN' && date !== '')
  if (currency === 'PLN') return null
  let text = 'Pobieranie kursu…'
  if (rate.isError) text = 'Nie udało się pobrać kursu. Spróbuj za chwilę.'
  if (rate.data) {
    const value = Number(rate.data.rate)
    const shown = value.toLocaleString('pl-PL', { maximumFractionDigits: 4 })
    text = `1 ${currency} = ${shown} zł (kurs EBC z ${formatDay(rate.data.rate_date)})`
    if (total)
      text += `, razem ≈ ${formatMoney(Math.round((total * value * 100) / 10 ** exponent))}`
  }
  return (
    <p className="-mt-4 text-sm text-muted-foreground" aria-live="polite">
      {text}
    </p>
  )
}

/** Who owes what, live, as the server will compute it. */
function SplitPreview({
  items,
  people,
  myId,
  currency,
}: {
  items: ExpenseItem[]
  people: UserBrief[]
  myId: number | undefined
  currency: string
}) {
  const money = useMoney()
  const owed = owedFor(items)
  const total = items.reduce((sum, item) => sum + item.amount, 0)
  if (total === 0 || owed.size === 0) return null
  return (
    <section className="flex flex-col gap-1 rounded-lg border bg-card px-4 py-3" aria-live="polite">
      <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
        Kto ile
      </h3>
      <ul className="divide-y">
        {people
          .filter((p) => owed.has(p.id))
          .map((p) => (
            <li key={p.id} className="flex items-center gap-3 py-1.5">
              <UserAvatar username={p.username} src={p.avatar_url} size="sm" />
              <span className="min-w-0 flex-1 truncate text-base">{who(p, myId)}</span>
              <span className="text-base font-semibold tabular-nums">
                {money.format(owed.get(p.id) ?? 0, currency)}
              </span>
            </li>
          ))}
      </ul>
    </section>
  )
}

const SPLITS: [SplitKind, string][] = [
  ['equal', 'Po równo'],
  ['shares', 'Udziałami'],
  ['exact', 'Kwotami'],
]

/** "2" -> 2; null for anything that is not a whole number of at least 1. */
const parseWeight = (text: string): number | null =>
  /^\d+$/.test(text.trim()) && Number(text) > 0 ? Number(text) : null

/** How one total is divided: evenly among the chosen people, by shares (e.g. 2:1 for a couple
 * and a single), or into exact amounts. Somebody with no share or amount isn't in it. */
function SplitField({
  people,
  myId,
  split,
  onSplit,
  sharedBy,
  onSharedBy,
  weights,
  onWeights,
  owed,
  total,
  currency,
  exponent,
}: {
  people: UserBrief[]
  myId: number
  split: SplitKind
  onSplit: (split: SplitKind) => void
  sharedBy: Set<number>
  onSharedBy: (next: Set<number>) => void
  weights: Map<number, string>
  onWeights: (next: Map<number, string>) => void
  owed: Map<number, number>
  total: number
  currency: string
  exponent: number
}) {
  const money = useMoney()
  const assigned = [...weights.values()].reduce((sum, t) => sum + (parseMoney(t, exponent) ?? 0), 0)
  const leftover = total - assigned
  // what is left, spread evenly over the people with nothing typed yet: their placeholders
  const empty = people.filter((p) => !(weights.get(p.id) ?? '').trim())
  const hints = new Map(
    leftover > 0
      ? allocate(
          leftover,
          empty.map(() => 1),
        ).map((minor, i) => [empty[i].id, minor] as const)
      : [],
  )
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1 text-base font-medium">Dla kogo</legend>
      <div className="grid grid-cols-3 gap-2" role="group" aria-label="Sposób podziału">
        {SPLITS.map(([key, label]) => (
          <Button
            key={key}
            type="button"
            size="sm"
            variant={split === key ? 'default' : 'outline'}
            aria-pressed={split === key}
            onClick={() => onSplit(key)}
          >
            {label}
          </Button>
        ))}
      </div>
      {split === 'equal' ? (
        <PeoplePicker people={people} selected={sharedBy} onChange={onSharedBy} label="Dla kogo" />
      ) : (
        <>
          <ul className="flex flex-col gap-2">
            {people.map((p) => (
              <li key={p.id} className="flex items-center gap-3">
                <UserAvatar username={p.username} src={p.avatar_url} size="sm" />
                <span className="min-w-0 flex-1 truncate text-base">{who(p, myId)}</span>
                {split === 'shares' && owed.has(p.id) && (
                  <span className="text-sm text-muted-foreground tabular-nums">
                    {money.format(owed.get(p.id) ?? 0, currency)}
                  </span>
                )}
                <Input
                  aria-label={
                    split === 'shares' ? `Udziały: ${p.username}` : `Kwota: ${p.username}`
                  }
                  className={split === 'shares' ? 'w-20' : 'w-32'}
                  inputMode={split === 'shares' ? 'numeric' : 'decimal'}
                  placeholder={
                    split === 'exact' ? toMoneyInput(hints.get(p.id) ?? 0, exponent) : '0'
                  }
                  value={weights.get(p.id) ?? ''}
                  onChange={(e) => onWeights(new Map(weights).set(p.id, e.target.value))}
                />
              </li>
            ))}
          </ul>
          {split === 'exact' && (
            <div className="flex flex-col items-start gap-1">
              <span className={cn('text-sm', leftover !== 0 && 'text-destructive')}>
                Rozdzielono {money.format(assigned, currency)} z {money.format(total, currency)}
                {leftover > 0 && `, zostało ${money.format(leftover, currency)}`}
                {leftover < 0 && `, o ${money.format(-leftover, currency)} za dużo`}
              </span>
              {hints.size > 0 && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="-ml-3"
                  onClick={() => {
                    const next = new Map(weights)
                    for (const [id, minor] of hints) {
                      if (minor > 0) next.set(id, toMoneyInput(minor, exponent))
                    }
                    onWeights(next)
                  }}
                >
                  Rozdziel resztę po równo
                  <WandSparkles data-icon="inline-end" aria-hidden />
                </Button>
              )}
            </div>
          )}
        </>
      )}
    </fieldset>
  )
}

interface ExpenseFormProps {
  meta: LedgerMeta
  /** Members (plus, when editing, anyone the expense mentions). */
  people: UserBrief[]
  myId: number
  /** The expense being edited; a new one otherwise. */
  entry?: ExpenseEntry
  pending: boolean
  error: unknown
  onSubmit: (input: ExpenseInput) => void
}

/**
 * A shared expense, Tricount-style: what it was, who paid and who it was for. Either one amount
 * split evenly, by shares or into exact amounts, or a receipt where every line is split evenly
 * among its own people.
 */
export function ExpenseForm({
  meta,
  people,
  myId,
  entry,
  pending,
  error,
  onSubmit,
}: ExpenseFormProps) {
  const exponentOf = (code: string) => meta.currencies.find((c) => c.code === code)?.exponent ?? 2
  const startExponent = exponentOf(entry?.currency ?? 'PLN')
  const asInput = (minor: number) => toMoneyInput(minor, startExponent)
  const startItems = entry?.details.items ?? []
  const startPayers = entry?.details.payers ?? []

  const [title, setTitle] = useState(entry?.title ?? '')
  const [currency, setCurrency] = useState(entry?.currency ?? meta.base_currency)
  const [date, setDate] = useState(entry?.occurred_on ?? todayInput())
  const [category, setCategory] = useState<LedgerCategory>(entry?.category || 'food')
  const [note, setNote] = useState(entry?.note ?? '')
  const [mode, setMode] = useState<Mode>(entry && isItemized(entry) ? 'items' : 'simple')
  const [amount, setAmount] = useState(entry ? asInput(entry.amount ?? 0) : '')
  const [sharedBy, setSharedBy] = useState<Set<number>>(
    () => new Set(entry ? startItems[0]?.shares.map((s) => s.user_id) : people.map((p) => p.id)),
  )
  // a single total can be split any way; a receipt's lines are always split evenly
  const single = startItems.length === 1 && mode === 'simple' ? startItems[0] : null
  const [split, setSplit] = useState<SplitKind>(single?.split ?? 'equal')
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
    startItems.map((item) => ({
      key: nextKey++,
      name: item.name,
      price: asInput(item.amount),
      people: new Set(item.shares.map((s) => s.user_id)),
      custom: item.split === 'equal' ? null : { split: item.split, shares: item.shares },
    })),
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
  const money = useMoney()
  const exponent = exponentOf(currency)

  /** Shares as typed for a split by shares or exact amounts (blank or zero: not in it). */
  const typedShares = [...weights]
    .flatMap(([id, text]) => {
      const weight = split === 'shares' ? parseWeight(text) : parseMoney(text, exponent)
      return weight ? [{ user_id: id, weight }] : []
    })
    .sort((a, b) => a.user_id - b.user_id)
  /** Who is in a single total right now, whichever way it is split. */
  const participants = (): Set<number> =>
    split === 'equal' ? sharedBy : new Set(typedShares.map((s) => s.user_id))

  // what the form says right now, in the API's shape
  const items: ExpenseItem[] =
    mode === 'simple'
      ? [
          {
            name: title.trim() || 'Wydatek',
            amount: parseMoney(amount, exponent) ?? 0,
            split,
            shares: split === 'equal' ? sharesOf(sharedBy) : typedShares,
          },
        ]
      : rows.map((row) => ({
          name: row.name.trim(),
          amount: parseMoney(row.price, exponent) ?? 0,
          split: row.custom?.split ?? 'equal',
          shares: row.custom?.shares ?? sharesOf(row.people),
        }))
  const total = items.reduce((sum, item) => sum + item.amount, 0)
  const paidBy = payers
    ? [...payers].map(([id, text]) => ({ user_id: id, amount: parseMoney(text, exponent) ?? 0 }))
    : [{ user_id: payer ?? myId, amount: total }]
  const paid = paidBy.reduce((sum, p) => sum + p.amount, 0)

  /** Switch how a single total is split, keeping the same people (exact amounts start even). */
  const changeSplit = (next: SplitKind) => {
    if (next === split) return
    const ids = [...participants()].sort((a, b) => a - b)
    if (next === 'equal') setSharedBy(new Set(ids))
    else if (next === 'shares') setWeights(new Map(ids.map((id) => [id, '1'])))
    else {
      const even = allocate(
        total,
        ids.map(() => 1),
      )
      setWeights(new Map(ids.map((id, i) => [id, toMoneyInput(even[i], exponent)])))
    }
    setSplit(next)
  }

  const problem = (): string | null => {
    if (!title.trim()) return 'Podaj, za co był wydatek.'
    if (mode === 'items' && rows.length === 0) return 'Dodaj co najmniej jedną pozycję.'
    if (mode === 'simple' && total === 0) return 'Podaj poprawną kwotę, np. 10 lub 12,50.'
    if (items.some((item) => !item.name || item.amount === 0)) {
      return 'Każda pozycja potrzebuje nazwy i poprawnej ceny.'
    }
    if (items.some((item) => item.shares.length === 0)) return 'Zaznacz, kogo dotyczy wydatek.'
    if (mode === 'simple' && split === 'exact') {
      const assigned = typedShares.reduce((sum, s) => sum + s.weight, 0)
      if (assigned !== total) {
        return `Kwoty osób muszą dać razem ${money.format(total, currency)}, a jest ${money.format(assigned, currency)}.`
      }
    }
    if (paid !== total) {
      return `Płacący muszą razem wyłożyć ${money.format(total, currency)}, a jest ${money.format(paid, currency)}.`
    }
    return null
  }

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const message = problem()
    setLocalError(message)
    if (message) return
    onSubmit({
      kind: 'expense',
      title: title.trim(),
      currency,
      occurred_on: date,
      category,
      note: note.trim(),
      items,
      payers: paidBy.filter((p) => p.amount > 0),
    })
  }

  return (
    <form onSubmit={submit} className="flex max-w-md flex-col gap-6">
      <div className="flex flex-col gap-2">
        <Label htmlFor="expense-title">Za co</Label>
        <Input
          id="expense-title"
          value={title}
          maxLength={200}
          placeholder="np. Pizza, Nocleg w Zakopanem"
          onChange={(e) => setTitle(e.target.value)}
          autoFocus={!entry}
        />
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="expense-date">Kiedy</Label>
          <Input
            id="expense-date"
            type="date"
            max={todayInput()}
            value={date}
            onChange={(e) => setDate(e.target.value)}
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="expense-category">Kategoria</Label>
          <select
            id="expense-category"
            className={SELECT}
            value={category}
            onChange={(e) => setCategory(e.target.value as LedgerCategory)}
          >
            {meta.categories.map((c) => (
              <option key={c.key} value={c.key}>
                {c.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2" role="group" aria-label="Rodzaj wydatku">
        {(
          [
            ['simple', 'Jedna kwota'],
            ['items', 'Z paragonu'],
          ] as const
        ).map(([key, label]) => (
          <Button
            key={key}
            type="button"
            variant={mode === key ? 'default' : 'outline'}
            aria-pressed={mode === key}
            onClick={() => {
              setMode(key)
              if (key === 'items' && rows.length === 0) setRows([newRow(participants())])
            }}
          >
            {label}
          </Button>
        ))}
      </div>

      <div className="flex gap-2">
        {mode === 'simple' ? (
          <div className="flex flex-1 flex-col gap-2">
            <Label htmlFor="expense-amount">Kwota</Label>
            <Input
              id="expense-amount"
              inputMode="decimal"
              placeholder={exponent ? '0,00' : '0'}
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
          </div>
        ) : (
          <div className="flex flex-1 flex-col gap-2">
            <span className="text-sm leading-none font-medium">Razem</span>
            <span className="flex h-12 items-center text-xl font-bold tabular-nums">
              {money.format(total, currency)}
            </span>
          </div>
        )}
        <div className="flex w-28 flex-col gap-2">
          <Label htmlFor="expense-currency">Waluta</Label>
          <select
            id="expense-currency"
            className={SELECT}
            value={currency}
            onChange={(e) => setCurrency(e.target.value)}
          >
            {meta.currencies.map((c) => (
              <option key={c.code} value={c.code}>
                {c.code}
              </option>
            ))}
          </select>
        </div>
      </div>
      <RateNote currency={currency} date={date} total={total} exponent={exponent} />

      <PayersField
        people={people}
        myId={myId}
        payer={payer}
        onPayer={setPayer}
        payers={payers}
        onPayers={setPayers}
        total={total}
        paid={paid}
        currency={currency}
        exponent={exponent}
      />

      {mode === 'simple' ? (
        <SplitField
          people={people}
          myId={myId}
          split={split}
          onSplit={changeSplit}
          sharedBy={sharedBy}
          onSharedBy={setSharedBy}
          weights={weights}
          onWeights={setWeights}
          owed={owedFor(items)}
          total={total}
          currency={currency}
          exponent={exponent}
        />
      ) : (
        <ItemsField
          rows={rows}
          onRows={setRows}
          people={people}
          fallbackPeople={sharedBy}
          exponent={exponent}
        />
      )}

      <SplitPreview items={items} people={people} myId={myId} currency={currency} />

      <details className="rounded-lg border bg-card px-4 py-3" open={Boolean(entry?.note)}>
        <summary className="cursor-pointer text-base font-medium">Notatka</summary>
        <Textarea
          className="mt-3"
          aria-label="Notatka"
          maxLength={500}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </details>

      {localError && (
        <span role="alert" className="text-base text-destructive">
          {localError}
        </span>
      )}
      <MutationError error={error} />
      <Button type="submit" size="lg" disabled={pending}>
        {pending ? 'Zapisywanie…' : entry ? 'Zapisz zmiany' : 'Dodaj wydatek'}
      </Button>
    </form>
  )
}
