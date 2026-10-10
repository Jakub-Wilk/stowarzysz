import { newRow, type Row } from '@/features/ledger/expense/rows'
import type { LedgerCurrency, ScannedReceipt } from '@/lib/api-types'
import { toMoneyInput } from '@/lib/money'

/** The priced lines of a scanned receipt as form rows (printed decimals to minor units). */
export function scannedRows(receipt: ScannedReceipt, exponent: number, people: number[]): Row[] {
  return (receipt.items ?? []).flatMap((item) => {
    const price =
      item.total ?? (item.unit_price != null ? item.unit_price * (item.quantity ?? 1) : null)
    const minor = price == null ? 0 : Math.round(price * 10 ** exponent)
    if (minor <= 0) return []
    const quantity = item.quantity != null && item.quantity !== 1 ? ` × ${item.quantity}` : ''
    return [
      newRow(people, {
        name: `${item.name?.trim() || 'Pozycja'}${quantity}`,
        price: toMoneyInput(minor, exponent),
      }),
    ]
  })
}

/** The usual currency of a receipt's language, for when the model could not read one. English is
 * left out on purpose: it says nothing about the country. */
const LANGUAGE_CURRENCY: Record<string, string> = {
  pl: 'PLN',
  de: 'EUR',
  fr: 'EUR',
  es: 'EUR',
  it: 'EUR',
  pt: 'EUR',
  nl: 'EUR',
  fi: 'EUR',
  el: 'EUR',
  sk: 'EUR',
  sl: 'EUR',
  et: 'EUR',
  lv: 'EUR',
  lt: 'EUR',
  hr: 'EUR',
  bg: 'EUR',
  cs: 'CZK',
  hu: 'HUF',
  sv: 'SEK',
  da: 'DKK',
  nb: 'NOK',
  no: 'NOK',
  ro: 'RON',
  tr: 'TRY',
  ja: 'JPY',
  ko: 'KRW',
  zh: 'CNY',
  he: 'ILS',
  th: 'THB',
  id: 'IDR',
}

/** The currency a scanned receipt is in, if the picker offers it: the one read off the receipt,
 * else a guess from its language. */
export function scannedCurrency(receipt: ScannedReceipt, offered: LedgerCurrency[]): string | null {
  const known = (code: string | null | undefined) => {
    const upper = code?.trim().toUpperCase()
    return upper && offered.some((c) => c.code === upper) ? upper : null
  }
  const language = receipt.language?.trim().toLowerCase().split(/[-_]/)[0]
  return known(receipt.currency) ?? known(language ? LANGUAGE_CURRENCY[language] : null)
}
