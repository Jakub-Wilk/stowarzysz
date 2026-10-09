import { SelectField } from '@/components/SelectField'
import { formatDay, useMoney } from '@/features/ledger/format'
import { useRate } from '@/features/ledger/hooks'
import type { LedgerMeta } from '@/lib/api-types'
import { formatMoney, toMoneyInput } from '@/lib/money'

/** The rate a foreign-currency amount will use, fetched (and cached server-side) while the user
 * types. */
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
    if (total) text += ` · ≈ ${formatMoney(Math.round((total * value * 100) / 10 ** exponent))}`
  }
  return (
    <p className="text-center text-sm text-muted-foreground" aria-live="polite">
      {text}
    </p>
  )
}

interface AmountFieldProps {
  id: string
  meta: LedgerMeta
  currency: string
  onCurrency: (currency: string) => void
  /** The day the rate is for. */
  date: string
  /** What the user typed; or, with `total` set and no `onAmount`, a computed, read-only sum. */
  amount: string
  onAmount?: (text: string) => void
  total: number
  /** For the read-only sum: what it is ("Suma pozycji"). */
  caption?: string
  autoFocus?: boolean
}

/** The amount, big and centred, the first thing a form asks: a number and its currency, plus
 * the exchange rate for a foreign one. */
export function AmountField({
  id,
  meta,
  currency,
  onCurrency,
  date,
  amount,
  onAmount,
  total,
  caption,
  autoFocus,
}: AmountFieldProps) {
  const money = useMoney()
  const exponent = money.exponent(currency)
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-center gap-2">
        {onAmount ? (
          <>
            <label htmlFor={id} className="sr-only">
              Kwota
            </label>
            <input
              id={id}
              inputMode="decimal"
              autoComplete="off"
              autoFocus={autoFocus}
              placeholder={exponent ? '0,00' : '0'}
              value={amount}
              onChange={(e) => onAmount(e.target.value)}
              className="h-16 w-full max-w-60 min-w-0 rounded-lg border-b-2 border-input bg-transparent text-center text-5xl font-bold tabular-nums transition-colors outline-none placeholder:text-muted-foreground/60 focus-visible:border-ring"
            />
          </>
        ) : (
          <output
            id={id}
            className="flex h-16 flex-col items-center justify-center text-5xl font-bold tabular-nums"
          >
            {toMoneyInput(total, exponent) || '0'}
          </output>
        )}
        <SelectField
          aria-label="Waluta"
          className="w-24 shrink-0"
          value={currency}
          onChange={onCurrency}
          options={meta.currencies.map((c) => ({ value: c.code, label: c.code }))}
        />
      </div>
      {caption && !onAmount && (
        <p className="text-center text-sm text-muted-foreground">{caption}</p>
      )}
      <RateNote currency={currency} date={date} total={total} exponent={exponent} />
    </div>
  )
}
