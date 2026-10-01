import { useEffect, useRef, useState } from 'react'

/**
 * True briefly after `key` changes (not on first render), for highlighting live updates such
 * as SSE-driven rows. Pair with the `animate-flash` utility.
 */
export function useFlashOnChange(key: unknown, ms = 1400): boolean {
  const previous = useRef(key)
  const [flashing, setFlashing] = useState(false)

  useEffect(() => {
    if (Object.is(previous.current, key)) return
    previous.current = key
    setFlashing(true)
    const timer = setTimeout(() => setFlashing(false), ms)
    return () => clearTimeout(timer)
  }, [key, ms])

  return flashing
}
