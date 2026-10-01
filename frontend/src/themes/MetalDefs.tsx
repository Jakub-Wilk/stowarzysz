/**
 * SVG strokes can't take a CSS gradient, so themes with a metallic look expose their stops as
 * --metal-1..5 and icons reference this shared gradient via `stroke: url(#metal)`.
 * Lucide icons use a 24x24 viewBox, hence user-space coordinates (works for flat, zero-height paths too).
 */
export function MetalDefs() {
  return (
    <svg width="0" height="0" className="absolute" aria-hidden focusable="false">
      <defs>
        <linearGradient id="metal" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="24">
          {[
            ['0%', 'var(--metal-1)'],
            ['30%', 'var(--metal-2)'],
            ['48%', 'var(--metal-3)'],
            ['66%', 'var(--metal-4)'],
            ['100%', 'var(--metal-5)'],
          ].map(([offset, color]) => (
            <stop key={offset} offset={offset} style={{ stopColor: color }} />
          ))}
        </linearGradient>
      </defs>
    </svg>
  )
}
