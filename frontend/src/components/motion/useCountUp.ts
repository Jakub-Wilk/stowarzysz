import { useEffect, useRef, useState } from 'react'

/** Tweens a number from its previous value to `value`. Jumps straight there under reduced motion. */
export function useCountUp(value: number, duration = 600): number {
  const [shown, setShown] = useState(0)
  const from = useRef(0)

  useEffect(() => {
    const start = from.current
    const t0 = performance.now()
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    let frame = 0
    const tick = (now: number) => {
      const t = reduced ? 1 : Math.min((now - t0) / duration, 1)
      const eased = 1 - (1 - t) ** 3
      const current = start + (value - start) * eased
      from.current = current
      setShown(current)
      if (t < 1) frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [value, duration])

  return Math.round(shown)
}
