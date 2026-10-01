import { useCallback, useState, type CSSProperties } from 'react'

interface Particle {
  id: number
  emoji: string
  /** Horizontal start, % of the viewport width. */
  x: number
  /** Sideways drift and spin over the animation. */
  drift: number
  rotation: number
}

let nextId = 0

/** Spawns single emoji "smoke" particles; render `layer` once on the screen. */
export function useSmoke() {
  const [particles, setParticles] = useState<Particle[]>([])

  const spawn = useCallback((emoji: string) => {
    const sign = Math.random() < 0.5 ? -1 : 1
    const particle: Particle = {
      id: nextId++,
      emoji,
      x: 10 + Math.random() * 80,
      drift: sign * (20 + Math.random() * 70),
      rotation: sign * (10 + Math.random() * 35),
    }
    setParticles((current) => [...current, particle])
  }, [])

  const remove = useCallback((id: number) => {
    setParticles((current) => current.filter((p) => p.id !== id))
  }, [])

  const layer = (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-40 overflow-hidden">
      {particles.map((p) => (
        <span
          key={p.id}
          className="animate-smoke absolute bottom-24 text-6xl select-none"
          style={
            {
              left: `${p.x}%`,
              '--smoke-drift': `${p.drift}px`,
              '--smoke-rot': `${p.rotation}deg`,
            } as CSSProperties
          }
          onAnimationEnd={() => remove(p.id)}
        >
          {p.emoji}
        </span>
      ))}
    </div>
  )

  return { spawn, layer }
}
