import type { CSSProperties } from 'react'

const WISPS = Array.from({ length: 9 }, (_, i) => ({
  x: 4 + ((i * 53) % 90),
  size: 50 + ((i * 17) % 50),
  duration: 10 + ((i * 7) % 9),
  delay: -((i * 5) % 17),
  drift: (i % 2 ? 1 : -1) * (1 + (i % 3)),
}))

const ORANGES = Array.from({ length: 4 }, (_, i) => ({
  bottom: 3 + ((i * 7) % 15),
  size: 22 + ((i * 5) % 10),
  duration: 55 + ((i * 13) % 40),
  delay: -((i * 23) % 70),
}))

/**
 * Decorative steam and floating oranges for the capybara theme. Transform/opacity only. Unlike the
 * other seasons' layers this sits behind the page (`-z-1`: above the water drawn by the theme's
 * `body::before`, below the capybara in `body::after` and all content), as part of the background.
 */
export function CapybaraLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 -z-1 overflow-hidden">
      {WISPS.map((w, i) => (
        <span
          key={`steam-${i}`}
          className="capy-steam"
          style={
            {
              left: `${w.x}%`,
              width: w.size,
              height: w.size,
              animationDelay: `${w.delay}s`,
              '--steam-duration': `${w.duration}s`,
              '--steam-drift': `${w.drift}vw`,
            } as CSSProperties
          }
        />
      ))}
      {ORANGES.map((o, i) => (
        <span
          key={`orange-${i}`}
          className="capy-orange"
          style={
            {
              bottom: `${o.bottom}vh`,
              width: o.size,
              height: o.size * 1.1,
              animationDelay: `${o.delay}s`,
              '--orange-duration': `${o.duration}s`,
            } as CSSProperties
          }
        />
      ))}
    </div>
  )
}
