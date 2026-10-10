import { QUARTERS } from '@/features/ledger/expense/useReceiptScan'

/** One continuous loading bar; the fill glides to the current quarter (transform only, so it
 * never reflows). */
export function ScanProgress({ quarters }: { quarters: number }) {
  return (
    <div
      role="progressbar"
      aria-label="Odczytywanie paragonu"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={(quarters / QUARTERS) * 100}
      className="h-2 overflow-hidden rounded-full bg-muted"
    >
      <div
        className="h-full origin-left rounded-full bg-primary transition-transform duration-700 ease-out"
        style={{ transform: `scaleX(${quarters / QUARTERS})` }}
      />
    </div>
  )
}
