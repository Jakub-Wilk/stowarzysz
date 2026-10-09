/** Amounts travel as integer grosze; people read and type złoty. */

/** 1050 -> "10,50 zł"; whole amounts drop the decimals: 1000 -> "10 zł". */
export function formatMoney(grosze: number): string {
  const sign = grosze < 0 ? '-' : ''
  const abs = Math.abs(grosze)
  const major = Math.floor(abs / 100)
  const minor = abs % 100
  const grouped = major.toLocaleString('pl-PL')
  return `${sign}${grouped}${minor ? `,${String(minor).padStart(2, '0')}` : ''} zł`
}

/** "10", "10,5" or "10.50" -> grosze; null for blank, zero, negative or invalid text. */
export function parseMoney(text: string): number | null {
  const cleaned = text.trim().replace(/\s/g, '').replace(',', '.')
  if (!/^\d+(\.\d{1,2})?$/.test(cleaned)) return null
  const grosze = Math.round(Number(cleaned) * 100)
  return grosze > 0 ? grosze : null
}
