/**
 * Amounts travel as integers in a currency's minor units (grosze for PLN, whole yen for JPY);
 * people read and type major units. `exponent` is the number of minor-unit digits (2, or 0 for
 * JPY); the ledger's `/meta/` lists it for every currency.
 */

/** 1050 -> "10,50 zł"; whole amounts drop the decimals: 1000 -> "10 zł", 1200 JPY -> "1 200 JPY". */
export function formatMoney(minor: number, currency = 'PLN', exponent = 2): string {
  const sign = minor < 0 ? '-' : ''
  const abs = Math.abs(minor)
  const unit = 10 ** exponent
  const major = Math.floor(abs / unit).toLocaleString('pl-PL')
  const rest = abs % unit
  const decimals = rest ? `,${String(rest).padStart(exponent, '0')}` : ''
  return `${sign}${major}${decimals} ${currency === 'PLN' ? 'zł' : currency}`
}

/** "10", "10,5" or "10.50" -> minor units; null for blank, zero, negative or invalid text. */
export function parseMoney(text: string, exponent = 2): number | null {
  const cleaned = text.trim().replace(/\s/g, '').replace(',', '.')
  const pattern = exponent > 0 ? new RegExp(`^\\d+(\\.\\d{1,${exponent}})?$`) : /^\d+$/
  if (!pattern.test(cleaned)) return null
  const minor = Math.round(Number(cleaned) * 10 ** exponent)
  return minor > 0 ? minor : null
}

/** Minor units back to what a person would type: 1050 -> "10,50", 1000 -> "10". */
export function toMoneyInput(minor: number, exponent = 2): string {
  const unit = 10 ** exponent
  const rest = minor % unit
  const major = Math.floor(minor / unit)
  return rest ? `${major},${String(rest).padStart(exponent, '0')}` : String(major)
}

/** Money (grosze) or goods (a quantity of an item): 1050 -> "10,50 zł", 3 + "piwo" -> "3 × piwo". */
export function formatDebt(debt: { amount: number; item: string }): string {
  if (!debt.item) return formatMoney(debt.amount)
  return debt.amount === 1 ? debt.item : `${debt.amount} × ${debt.item}`
}
