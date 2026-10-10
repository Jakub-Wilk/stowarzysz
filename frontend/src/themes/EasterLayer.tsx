import type { CSSProperties } from 'react'

const COLORS = [
  'oklch(0.85 0.1 350)',
  'oklch(0.9 0.12 95)',
  'oklch(0.86 0.1 150)',
  'oklch(0.82 0.1 310)',
  'oklch(0.84 0.09 235)',
]

const EGGS = Array.from({ length: 14 }, (_, i) => ({
  x: (i * 47) % 100,
  width: 10 + ((i * 3) % 5),
  duration: 15 + ((i * 7) % 12),
  delay: -((i * 5) % 21),
  drift: (i % 2 ? 1 : -1) * (2 + (i % 4)),
  color: COLORS[i % COLORS.length],
}))

/** Decorative pastel eggs for the Easter theme. Transform/opacity only. */
export function EasterLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
      {EGGS.map((e, i) => (
        <span
          key={i}
          className="easter-egg"
          style={
            {
              left: `${e.x}%`,
              width: e.width,
              height: e.width * 1.3,
              animationDelay: `${e.delay}s`,
              '--egg-duration': `${e.duration}s`,
              '--egg-drift': `${e.drift}vw`,
              '--egg-color': e.color,
            } as CSSProperties
          }
        />
      ))}
    </div>
  )
}
