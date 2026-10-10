import type { CSSProperties } from 'react'

const FLAKES = Array.from({ length: 40 }, (_, i) => ({
  x: (i * 37) % 100,
  size: 2 + ((i * 5) % 7),
  // Big flakes are nearer: they fall faster and are softly out of focus.
  duration: 20 - ((i * 5) % 7) * 1.5 + ((i * 7) % 5),
  delay: -((i * 3) % 17),
  drift: ((i % 2 ? 1 : -1) * (2 + (i % 4))) as number,
}))

/** Decorative snowfall for the Christmas theme. Transform/opacity only; paused by reduced motion. */
export function SnowLayer() {
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
      {FLAKES.map((f, i) => (
        <span
          key={i}
          className="animate-snowfall absolute top-0 rounded-full bg-white"
          style={
            {
              left: `${f.x}%`,
              width: f.size,
              height: f.size,
              animationDelay: `${f.delay}s`,
              filter: f.size >= 6 ? 'blur(1.2px)' : undefined,
              '--snow-duration': `${f.duration}s`,
              '--snow-drift': `${f.drift}vw`,
            } as CSSProperties
          }
        />
      ))}
    </div>
  )
}
