import type { CSSProperties } from 'react'

const BATS = Array.from({ length: 6 }, (_, i) => ({
  top: 8 + ((i * 23) % 55),
  width: 22 + ((i * 7) % 14),
  duration: 16 + ((i * 5) % 13),
  delay: -((i * 7) % 23),
  bob: (i % 2 ? 1 : -1) * (14 + ((i * 3) % 18)),
}))

const EMBERS = Array.from({ length: 16 }, (_, i) => ({
  x: (i * 41) % 100,
  size: 2 + ((i * 3) % 4),
  duration: 9 + ((i * 5) % 9),
  delay: -((i * 4) % 13),
  drift: (i % 2 ? 1 : -1) * (1 + (i % 4)),
}))

/** Decorative bats and rising embers for the Halloween theme. Transform/opacity only. */
export function HalloweenLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
      {EMBERS.map((e, i) => (
        <span
          key={`ember-${i}`}
          className="halloween-ember"
          style={
            {
              left: `${e.x}%`,
              width: e.size,
              height: e.size,
              animationDelay: `${e.delay}s`,
              '--ember-duration': `${e.duration}s`,
              '--ember-drift': `${e.drift}vw`,
            } as CSSProperties
          }
        />
      ))}
      {BATS.map((b, i) => (
        <span
          key={`bat-${i}`}
          className="halloween-bat"
          style={
            {
              top: `${b.top}%`,
              animationDelay: `${b.delay}s`,
              '--bat-duration': `${b.duration}s`,
              '--bat-bob': `${b.bob}px`,
            } as CSSProperties
          }
        >
          <svg viewBox="0 0 32 16" width={b.width} height={b.width / 2}>
            <path d="M16 4 18 2 19 5C22 3 27 3 31 6 28 6 27 8 26 10 24 8 22 8 20 10 19 9 18 11 16 15 14 11 13 9 12 10 10 8 8 8 6 10 5 8 4 6 1 6 5 3 10 3 13 5L14 2Z" />
          </svg>
        </span>
      ))}
    </div>
  )
}
