import { Minus, Plus } from 'lucide-react'
import { useRef, type KeyboardEvent, type PointerEvent } from 'react'

import { Button } from '@/components/ui/button'
import { toneClasses } from '@/features/voting/tone'
import { cn } from '@/lib/utils'

export const MIN = -5
export const MAX = 5

const WORDS: Record<number, string> = {
  [-5]: 'Hańba',
  [-4]: 'Zdrada',
  [-3]: 'Sprzeciw',
  [-2]: 'Niezbyt',
  [-1]: 'Raczej nie',
  0: 'Obojętnie',
  1: 'Ujdzie',
  2: 'Może być',
  3: 'Popieram',
  4: 'Brawo',
  5: 'Owacja',
}

// Geometry of the half-circle, in SVG user units (viewBox is 300 x 190).
const CX = 150
const CY = 150
const R = 118
const VIEW_W = 300
const VIEW_H = 190

/** -5 sits on the left end of the arc, +5 on the right end, 0 at the top. */
const angleOf = (v: number) => Math.PI * (1 - (v - MIN) / (MAX - MIN))
const pointAt = (v: number, radius = R) => ({
  x: CX + radius * Math.cos(angleOf(v)),
  y: CY - radius * Math.sin(angleOf(v)),
})

const toneOf = (v: number | null) =>
  v === null ? 'neutral' : v > 0 ? 'positive' : v < 0 ? 'negative' : 'neutral'
const colorOf = (v: number | null) => `var(--${toneOf(v)})`

function arc(from: number, to: number): string {
  const a = pointAt(from)
  const b = pointAt(to)
  const sweep = to > from ? 1 : 0 // left to right is clockwise on screen
  return `M ${a.x} ${a.y} A ${R} ${R} 0 0 ${sweep} ${b.x} ${b.y}`
}

interface ScoreDialProps {
  value: number | null
  onChange: (value: number) => void
  disabled?: boolean
}

/**
 * A half-circle dial from -5 to +5: drag (or tap) anywhere on it, or use the +/- buttons.
 * The arc fills outward from the centre in the colour of the side you are leaning to.
 */
export function ScoreDial({ value, onChange, disabled }: ScoreDialProps) {
  const svg = useRef<SVGSVGElement>(null)
  const dragging = useRef(false)

  const set = (next: number) => {
    const clamped = Math.min(MAX, Math.max(MIN, next))
    if (clamped === value) return
    onChange(clamped)
    navigator.vibrate?.(8) // a small tick on phones that support it
  }

  const fromPointer = (e: PointerEvent<SVGSVGElement>) => {
    const box = svg.current?.getBoundingClientRect()
    if (!box) return
    const x = ((e.clientX - box.left) / box.width) * VIEW_W
    const y = ((e.clientY - box.top) / box.height) * VIEW_H
    // Below the centre line there is no arc: clamp to the nearer end.
    const angle = y >= CY ? (x < CX ? Math.PI : 0) : Math.atan2(CY - y, x - CX)
    set(Math.round((1 - angle / Math.PI) * (MAX - MIN) + MIN))
  }

  const onKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    const step = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1 }[e.key]
    if (step !== undefined) set((value ?? 0) + step)
    else if (e.key === 'Home') set(MIN)
    else if (e.key === 'End') set(MAX)
    else return
    e.preventDefault()
  }

  const tone = toneClasses(toneOf(value))
  const knob = value === null ? null : pointAt(value)

  return (
    <div className="flex flex-col items-center gap-2">
      <div className="relative w-full max-w-sm animate-pop-in">
        <svg
          ref={svg}
          viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
          role="slider"
          tabIndex={disabled ? -1 : 0}
          aria-label="Twoja ocena"
          aria-valuemin={MIN}
          aria-valuemax={MAX}
          aria-valuenow={value ?? undefined}
          aria-valuetext={value === null ? 'Brak oceny' : `${value}, ${WORDS[value]}`}
          aria-disabled={disabled}
          className="block w-full touch-none rounded-2xl outline-none select-none focus-visible:ring-3 focus-visible:ring-ring/50"
          onKeyDown={onKeyDown}
          onPointerDown={(e) => {
            if (disabled) return
            dragging.current = true
            e.currentTarget.setPointerCapture(e.pointerId)
            fromPointer(e)
          }}
          onPointerMove={(e) => dragging.current && fromPointer(e)}
          onPointerUp={() => (dragging.current = false)}
          onPointerCancel={() => (dragging.current = false)}
        >
          <defs>
            <linearGradient id="dial-track" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" style={{ stopColor: 'var(--negative)' }} />
              <stop offset="50%" style={{ stopColor: 'var(--neutral)' }} />
              <stop offset="100%" style={{ stopColor: 'var(--positive)' }} />
            </linearGradient>
          </defs>

          {/* the scale: a faint red -> gray -> green track */}
          <path
            d={arc(MIN, MAX)}
            fill="none"
            stroke="url(#dial-track)"
            strokeWidth="26"
            strokeLinecap="round"
            opacity="0.28"
          />
          {/* your side, filled from the centre outward */}
          {value !== null && value !== 0 && (
            <path
              d={arc(0, value)}
              fill="none"
              stroke={colorOf(value)}
              strokeWidth="26"
              strokeLinecap="round"
              style={{ transition: 'stroke 150ms' }}
            />
          )}
          {/* ticks, with the centre tick emphasised */}
          {Array.from({ length: MAX - MIN + 1 }, (_, i) => MIN + i).map((v) => {
            const outer = pointAt(v, R + 24)
            const inner = pointAt(v, R + (v === 0 ? 14 : 18))
            return (
              <line
                key={v}
                x1={inner.x}
                y1={inner.y}
                x2={outer.x}
                y2={outer.y}
                stroke="currentColor"
                strokeWidth={v === 0 ? 3 : 2}
                strokeLinecap="round"
                opacity={v === 0 ? 0.9 : 0.4}
              />
            )
          })}
          {/* end labels */}
          <text
            x={pointAt(MIN).x}
            y={CY + 30}
            textAnchor="middle"
            className="fill-negative text-[15px] font-semibold"
          >
            −5
          </text>
          <text
            x={pointAt(MAX).x}
            y={CY + 30}
            textAnchor="middle"
            className="fill-positive text-[15px] font-semibold"
          >
            +5
          </text>
          {/* the knob */}
          {knob && (
            <g
              style={{
                transform: `translate(${knob.x}px, ${knob.y}px)`,
                transition: 'transform 140ms ease-out',
              }}
            >
              <circle
                r="21"
                fill="var(--background)"
                stroke={colorOf(value)}
                strokeWidth="5"
                style={{ transition: 'stroke 150ms' }}
              />
              <circle r="8" fill={colorOf(value)} style={{ transition: 'fill 150ms' }} />
            </g>
          )}
        </svg>

        {/* the readout sits inside the half-circle */}
        <div className="pointer-events-none absolute inset-x-0 top-[40%] flex flex-col items-center">
          <span
            className={cn(
              'text-6xl leading-none font-black tabular-nums',
              value === null ? 'text-muted-foreground' : tone.text,
            )}
          >
            {value === null ? '–' : value > 0 ? `+${value}` : value}
          </span>
          <span className="mt-1 text-base text-muted-foreground">
            {value === null ? 'Przeciągnij pokrętło' : WORDS[value]}
          </span>
        </div>
      </div>

      <div className="flex w-full max-w-sm items-center justify-between">
        <Button
          type="button"
          variant="outline"
          size="icon-lg"
          aria-label="Zmniejsz ocenę"
          disabled={disabled || value === MIN}
          onClick={() => set((value ?? 0) - 1)}
        >
          <Minus />
        </Button>
        <span className="text-sm text-muted-foreground">
          Przeciągnij, dotknij lub zmieniaj krokami
        </span>
        <Button
          type="button"
          variant="outline"
          size="icon-lg"
          aria-label="Zwiększ ocenę"
          disabled={disabled || value === MAX}
          onClick={() => set((value ?? 0) + 1)}
        >
          <Plus />
        </Button>
      </div>
    </div>
  )
}
