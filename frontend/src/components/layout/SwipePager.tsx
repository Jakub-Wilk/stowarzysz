import { useRef, useState, type PointerEvent } from 'react'

import { cn } from '@/lib/utils'

const AXIS_LOCK_PX = 10
const DISTANCE_RATIO = 0.25
const FLICK_PX_PER_MS = 0.4
const EDGE_RESISTANCE = 0.3

interface Gesture {
  id: number
  startX: number
  startY: number
  startT: number
  axis: 'x' | 'y' | null
}

interface SwipePagerProps {
  index: number
  onIndexChange: (index: number) => void
  children: React.ReactNode[]
}

/** Horizontal pager: pages sit side by side, follow the finger while dragging, then slide into place. */
export function SwipePager({ index, onIndexChange, children }: SwipePagerProps) {
  const last = children.length - 1
  const gesture = useRef<Gesture | null>(null)
  const [dragX, setDragX] = useState<number | null>(null)

  const end = (e: PointerEvent<HTMLDivElement>, commit: boolean) => {
    const g = gesture.current
    gesture.current = null
    if (!g || g.id !== e.pointerId) return
    if (g.axis === 'x' && commit) {
      const dx = e.clientX - g.startX
      const width = e.currentTarget.clientWidth
      const velocity = dx / Math.max(performance.now() - g.startT, 1)
      const flicked = Math.abs(velocity) > FLICK_PX_PER_MS
      if (flicked || Math.abs(dx) > width * DISTANCE_RATIO) {
        const next = Math.min(Math.max(index + (dx < 0 ? 1 : -1), 0), last)
        if (next !== index) onIndexChange(next)
      }
    }
    setDragX(null)
  }

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.pointerType === 'mouse' || gesture.current) return
    gesture.current = {
      id: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      startT: performance.now(),
      axis: null,
    }
  }

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const g = gesture.current
    if (!g || g.id !== e.pointerId) return
    const dx = e.clientX - g.startX
    const dy = e.clientY - g.startY
    if (g.axis === null) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < AXIS_LOCK_PX) return
      g.axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
      if (g.axis === 'x') e.currentTarget.setPointerCapture(e.pointerId)
    }
    if (g.axis !== 'x') return
    const pastEdge = (index === 0 && dx > 0) || (index === last && dx < 0)
    setDragX(pastEdge ? dx * EDGE_RESISTANCE : dx)
  }

  const dragging = dragX !== null
  return (
    <div
      className="flex-1 touch-pan-y overflow-x-hidden"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={(e) => end(e, true)}
      onPointerCancel={(e) => end(e, false)}
    >
      <div
        className={cn(
          'flex items-start',
          !dragging && 'transition-transform duration-300 ease-out motion-reduce:transition-none',
        )}
        style={{ transform: `translateX(calc(${-index * 100}% + ${dragX ?? 0}px))` }}
      >
        {children.map((child, i) => (
          <div key={i} className="w-full shrink-0 p-4" inert={i !== index}>
            {child}
          </div>
        ))}
      </div>
    </div>
  )
}
