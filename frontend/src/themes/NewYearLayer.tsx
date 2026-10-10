import type { CSSProperties } from 'react'

const COLORS = [
  'oklch(0.88 0.14 88)',
  'oklch(0.95 0.03 250)',
  'oklch(0.72 0.2 350)',
  'oklch(0.78 0.13 210)',
  'oklch(0.72 0.18 300)',
]

const SPARKS = 16

const BURSTS = Array.from({ length: 7 }, (_, i) => ({
  x: 8 + ((i * 29) % 84),
  y: 6 + ((i * 17) % 38),
  radius: 44 + ((i * 13) % 34),
  duration: 8 + ((i * 3) % 5),
  delay: -((i * 5) % 11),
  color: COLORS[i % COLORS.length],
}))

/**
 * Decorative fireworks for the New Year's Eve theme. Transform/opacity only. Like the capybara
 * layer it sits behind the page (`-z-1`: above the skyline-less sky, below the skyline's windows
 * and all content), so it is part of the background and does not cover the app.
 */
export function NewYearLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 -z-1 overflow-hidden">
      {BURSTS.map((b, i) => (
        <span
          key={i}
          className="newyear-burst"
          style={
            {
              left: `${b.x}%`,
              top: `${b.y}%`,
              '--burst-duration': `${b.duration}s`,
              '--burst-delay': `${b.delay}s`,
              '--spark': b.color,
              '--radius': `${b.radius}px`,
            } as CSSProperties
          }
        >
          {Array.from({ length: SPARKS }, (_, k) => (
            <span
              key={k}
              className="newyear-spark"
              style={{ '--angle': `${(360 / SPARKS) * k}deg` } as CSSProperties}
            />
          ))}
        </span>
      ))}
    </div>
  )
}
