import { useEffect, useState } from 'react'

/** The current time, refreshed every 30 s while `active`, so countdowns stay accurate. */
export function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!active) return
    const timer = setInterval(() => setNow(Date.now()), 30_000)
    return () => clearInterval(timer)
  }, [active])
  return now
}
