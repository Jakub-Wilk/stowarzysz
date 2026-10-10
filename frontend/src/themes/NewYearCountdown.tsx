import { useState, useSyncExternalStore } from 'react'

import { useTheme } from '@/themes/context'

const subscribe = (onTick: () => void): (() => void) => {
  const id = setInterval(onTick, 1000)
  return () => clearInterval(id)
}
/** The current time in whole seconds: a number, so the store only changes once a second. */
const nowInSeconds = (): number => Math.floor(Date.now() / 1000)

/** The next local midnight, in epoch seconds. */
function nextMidnight(): number {
  const midnight = new Date()
  midnight.setHours(24, 0, 0, 0)
  return Math.floor(midnight.getTime() / 1000)
}

const pad = (n: number): string => String(n).padStart(2, '0')

function Countdown() {
  // Fixed when the header mounts: at zero it stays on the greeting instead of starting a new day.
  const [midnight] = useState(nextMidnight)
  const now = useSyncExternalStore(subscribe, nowInSeconds, nowInSeconds)
  const left = Math.max(0, midnight - now)
  const hours = Math.floor(left / 3600)
  const minutes = Math.floor((left % 3600) / 60)
  // Fewer units as midnight nears: hours go under an hour, minutes and the label under a minute,
  // when the seconds grow to fill the header. The tiles keep their keys, so the seconds tile
  // survives the changes and its growth animates.
  const lastMinute = left < 60

  return (
    <div
      role="timer"
      aria-label="Odliczanie do północy"
      data-final={left > 0 && left <= 10 ? '' : undefined}
      className="newyear-countdown flex shrink-0 flex-col items-center min-[480px]:absolute min-[480px]:left-1/2 min-[480px]:-translate-x-1/2"
    >
      {left === 0 ? (
        <span className="newyear-countdown-greeting">Z Nowym Rokiem!</span>
      ) : (
        <>
          {!lastMinute && <span className="newyear-countdown-label">do północy</span>}
          <span className="newyear-countdown-clock">
            {hours > 0 && <b key="h">{pad(hours)}</b>}
            {hours > 0 && <i key="hc">:</i>}
            {!lastMinute && <b key="m">{pad(minutes)}</b>}
            {!lastMinute && <i key="mc">:</i>}
            <b key="s" data-big={lastMinute ? '' : undefined}>
              {pad(left % 60)}
            </b>
          </span>
        </>
      )}
    </div>
  )
}

/**
 * A live countdown to midnight for the header, shown only while the New Year's Eve theme is on.
 * Centred on the screen when it is wide enough to clear the logo; on a phone it sits between the
 * logo and the avatar instead.
 */
export function NewYearCountdown() {
  const { theme } = useTheme()
  return theme === 'newyear' ? <Countdown /> : null
}
