import type { CSSProperties } from 'react'

const PETALS = Array.from({ length: 22 }, (_, i) => ({
  x: (i * 43) % 100,
  size: 7 + ((i * 5) % 7),
  duration: 11 + ((i * 7) % 11),
  delay: -((i * 5) % 19),
  drift: (i % 2 ? 1 : -1) * (2 + (i % 5)),
  spin: (i % 3 ? 1 : -1) * (300 + ((i * 47) % 300)),
  white: i % 3 === 1,
}))

/** Decorative red and white petals for the Polish theme. Transform/opacity only. */
export function PolandLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
      {PETALS.map((p, i) => (
        <span
          key={i}
          className="polska-petal"
          data-white={p.white || undefined}
          style={
            {
              left: `${p.x}%`,
              width: p.size,
              height: p.size,
              animationDelay: `${p.delay}s`,
              '--petal-duration': `${p.duration}s`,
              '--petal-drift': `${p.drift}vw`,
              '--petal-spin': `${p.spin}deg`,
            } as CSSProperties
          }
        />
      ))}
    </div>
  )
}
