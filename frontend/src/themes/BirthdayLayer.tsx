import type { CSSProperties } from 'react'

const COLORS = [
  'oklch(0.7 0.2 350)',
  'oklch(0.85 0.16 90)',
  'oklch(0.72 0.14 235)',
  'oklch(0.72 0.16 300)',
  'oklch(0.76 0.15 160)',
]

const BALLOONS = Array.from({ length: 12 }, (_, i) => ({
  x: (i * 53) % 96,
  width: 26 + ((i * 7) % 14),
  duration: 16 + ((i * 5) % 12),
  delay: -((i * 7) % 23),
  drift: (i % 2 ? 1 : -1) * (2 + (i % 4)),
  color: COLORS[i % COLORS.length],
}))

/** Decorative balloons floating up for the birthday theme. Transform/opacity only. */
export function BirthdayLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
      {BALLOONS.map((b, i) => (
        <span
          key={i}
          className="birthday-balloon"
          style={
            {
              left: `${b.x}%`,
              width: b.width,
              height: b.width * 1.2,
              animationDelay: `${b.delay}s`,
              '--balloon-duration': `${b.duration}s`,
              '--balloon-drift': `${b.drift}vw`,
              '--balloon-color': b.color,
            } as CSSProperties
          }
        />
      ))}
    </div>
  )
}
